//! Visual signatures and fill-in content: placing signature images, drawn (ink) signatures and
//! text on pages, either as real annotations with appearance streams (editable in Acrobat and
//! other viewers) or flattened into the page content.
//!
//! Coordinates: placements are given in *view space* — points (1/72 in), origin at the top-left
//! of the page as displayed (after /Rotate), y growing downwards. The geometry here converts them
//! to PDF user space, honouring CropBox/MediaBox origin and page rotation.
//!
//! Cryptographic (PKCS#7/CMS) signing is phase 2; see `docs/signature-embedding.md`
//! and [`byte_range`] for the reserved placeholder logic.

use std::fmt;

use image::RgbaImage;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat};

use crate::build::{array, deref, dict, int};

#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    /// (x, y, pressure 0..1) in the signature's own drawing space.
    pub points: Vec<[f32; 3]>,
}

#[derive(Debug, Clone)]
pub enum Content {
    /// A raster signature with alpha (typed or imported).
    Image(RgbaImage),
    /// A drawn signature kept as vector strokes. `view` is the size of the drawing space.
    Ink { strokes: Vec<Stroke>, view: (f32, f32), width: f32, color: [f32; 3] },
    /// Date / free text, Helvetica.
    Text { text: String, font_size: f32, color: [f32; 3] },
}

#[derive(Debug, Clone)]
pub struct Placement {
    /// 0-based page index in the document.
    pub page: usize,
    /// Box in view space (points): top-left corner and size, before rotation.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Clockwise rotation in degrees about the box centre.
    pub rotation: f32,
    pub content: Content,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Stamp / Ink / FreeText annotations with appearance streams.
    Annotate,
    /// Bake into the page content stream.
    Flatten,
}

#[derive(Debug, Default)]
pub struct ApplyReport {
    pub applied: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum SignError {
    PageOutOfRange(usize),
    BadPlacement(String),
    Pdf(String),
}

impl fmt::Display for SignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PageOutOfRange(p) => write!(f, "Page {} does not exist.", p + 1),
            Self::BadPlacement(m) => write!(f, "Invalid placement: {m}"),
            Self::Pdf(m) => write!(f, "Could not modify the PDF: {m}"),
        }
    }
}
impl std::error::Error for SignError {}

// ---------------------------------------------------------------- geometry

/// Page box and rotation, used to convert between view space and PDF user space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageGeom {
    pub x0: f32,
    pub y0: f32,
    pub w0: f32,
    pub h0: f32,
    /// 0, 90, 180 or 270, clockwise.
    pub rot: i32,
}

impl PageGeom {
    /// Page size as displayed (rotation applied).
    pub fn view_size(&self) -> (f32, f32) {
        if self.rot % 180 == 0 { (self.w0, self.h0) } else { (self.h0, self.w0) }
    }

    /// View space (top-left origin, rotated) -> PDF user space.
    pub fn view_to_pdf(&self, vx: f32, vy: f32) -> (f32, f32) {
        let (xp, yp) = match self.rot {
            90 => (vy, vx),
            180 => (self.w0 - vx, vy),
            270 => (self.w0 - vy, self.h0 - vx),
            _ => (vx, self.h0 - vy),
        };
        (self.x0 + xp, self.y0 + yp)
    }
}

fn inherited(doc: &Document, page: ObjectId, key: &[u8]) -> Option<Object> {
    let mut cur = Some(page);
    for _ in 0..64 {
        let id = cur?;
        let Some(Object::Dictionary(d)) = doc.objects.get(&id) else { return None };
        if let Ok(v) = d.get(key) {
            return deref(doc, v).cloned();
        }
        cur = match d.get(b"Parent") {
            Ok(Object::Reference(r)) => Some(*r),
            _ => None,
        };
    }
    None
}

fn num(o: &Object) -> Option<f32> {
    match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

pub fn page_geom(doc: &Document, page: ObjectId) -> Result<PageGeom, SignError> {
    let boxv = inherited(doc, page, b"CropBox").or_else(|| inherited(doc, page, b"MediaBox"));
    let arr = match &boxv {
        Some(Object::Array(a)) if a.len() == 4 => a.iter().filter_map(|o| deref(doc, o).and_then(num)).collect::<Vec<_>>(),
        _ => vec![0.0, 0.0, 612.0, 792.0],
    };
    if arr.len() != 4 {
        return Err(SignError::Pdf("malformed page box".into()));
    }
    let (x0, x1) = (arr[0].min(arr[2]), arr[0].max(arr[2]));
    let (y0, y1) = (arr[1].min(arr[3]), arr[1].max(arr[3]));
    let rot = inherited(doc, page, b"Rotate").and_then(|o| int(&o)).unwrap_or(0).rem_euclid(360) as i32;
    let rot = (rot / 90) * 90;
    if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
        return Err(SignError::Pdf("empty page box".into()));
    }
    Ok(PageGeom { x0, y0, w0: x1 - x0, h0: y1 - y0, rot })
}

/// Maps the placement's local space (origin bottom-left of the unrotated box, y up, units = points)
/// to PDF user space: `[a b c d e f]`.
pub fn local_matrix(g: &PageGeom, p: &Placement) -> [f32; 6] {
    let (cx, cy) = (p.x + p.w / 2.0, p.y + p.h / 2.0);
    let th = p.rotation.to_radians();
    let (s, c) = th.sin_cos();
    let to_pdf = |lx: f32, ly: f32| -> (f32, f32) {
        let (vx, vy) = (p.x + lx, p.y + (p.h - ly));
        let (dx, dy) = (vx - cx, vy - cy);
        g.view_to_pdf(cx + dx * c - dy * s, cy + dx * s + dy * c)
    };
    let o = to_pdf(0.0, 0.0);
    let px = to_pdf(p.w, 0.0);
    let py = to_pdf(0.0, p.h);
    [(px.0 - o.0) / p.w, (px.1 - o.1) / p.w, (py.0 - o.0) / p.h, (py.1 - o.1) / p.h, o.0, o.1]
}

fn apply_matrix(m: &[f32; 6], x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn bbox_in_page(m: &[f32; 6], w: f32, h: f32) -> [f32; 4] {
    let pts = [apply_matrix(m, 0.0, 0.0), apply_matrix(m, w, 0.0), apply_matrix(m, w, h), apply_matrix(m, 0.0, h)];
    let xs = pts.iter().map(|p| p.0);
    let ys = pts.iter().map(|p| p.1);
    [
        xs.clone().fold(f32::MAX, f32::min),
        ys.clone().fold(f32::MAX, f32::min),
        xs.fold(f32::MIN, f32::max),
        ys.fold(f32::MIN, f32::max),
    ]
}

// ---------------------------------------------------------------- content generation

fn real(v: f32) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" || s == "-0" { "0".into() } else { s.to_string() }
}

/// Encode text for a WinAnsi Helvetica string. Unsupported characters become '?'.
fn win_ansi(text: &str, warn: &mut bool) -> Vec<u8> {
    let mut out = Vec::new();
    for ch in text.chars() {
        let b = match ch as u32 {
            0x20..=0x7E | 0xA0..=0xFF => ch as u32 as u8,
            0x20AC => 0x80,
            0x2013 => 0x96,
            0x2014 => 0x97,
            0x2018 => 0x91,
            0x2019 => 0x92,
            0x201C => 0x93,
            0x201D => 0x94,
            0x2022 => 0x95,
            _ => {
                *warn = true;
                b'?'
            }
        };
        match b {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(b);
            }
            _ => out.push(b),
        }
    }
    out
}

struct Built {
    /// Content stream in local space.
    content: Vec<u8>,
    image: Option<ObjectId>,
    uses_font: bool,
}

fn rgb(c: [f32; 3]) -> String {
    format!("{} {} {}", real(c[0].clamp(0.0, 1.0)), real(c[1].clamp(0.0, 1.0)), real(c[2].clamp(0.0, 1.0)))
}

fn build_content(
    doc: &mut Document,
    p: &Placement,
    img_name: &str,
    report: &mut ApplyReport,
) -> Result<Built, SignError> {
    let (w, h) = (p.w, p.h);
    match &p.content {
        Content::Image(im) => {
            let id = add_image(doc, im)?;
            let content = format!("q {} 0 0 {} 0 0 cm /{img_name} Do Q", real(w), real(h)).into_bytes();
            Ok(Built { content, image: Some(id), uses_font: false })
        }
        Content::Text { text, font_size, color } => {
            let mut warn = false;
            let bytes = win_ansi(text, &mut warn);
            if warn {
                report.warnings.push("Some characters are not supported by the standard font and were replaced with '?'.".into());
            }
            let fs = *font_size;
            let baseline = ((h - fs) / 2.0 + fs * 0.22).max(0.0);
            let mut c = format!("BT /PFHelv {} Tf {} rg 0 {} Td (", real(fs), rgb(*color), real(baseline)).into_bytes();
            c.extend_from_slice(&bytes);
            c.extend_from_slice(b") Tj ET");
            Ok(Built { content: c, image: None, uses_font: true })
        }
        Content::Ink { strokes, view, width, color } => {
            if view.0 <= 0.0 || view.1 <= 0.0 {
                return Err(SignError::BadPlacement("empty drawing".into()));
            }
            let (sx, sy) = (w / view.0, h / view.1);
            let mut c = format!("1 J 1 j {} RG\n", rgb(*color)).into_bytes();
            let map = |pt: &[f32; 3]| (pt[0] * sx, h - pt[1] * sy);
            for s in strokes.iter().filter(|s| !s.points.is_empty()) {
                if s.points.len() == 1 {
                    let (x, y) = map(&s.points[0]);
                    c.extend_from_slice(format!("{} w {} {} m {} {} l S\n", real(width * sx), real(x), real(y), real(x + 0.01), real(y)).as_bytes());
                    continue;
                }
                for win in s.points.windows(2) {
                    let (a, b) = (map(&win[0]), map(&win[1]));
                    let pr = (win[0][2] + win[1][2]) / 2.0;
                    let lw = width * sx * (0.45 + 1.1 * pr.clamp(0.0, 1.0));
                    c.extend_from_slice(format!("{} w {} {} m {} {} l S\n", real(lw), real(a.0), real(a.1), real(b.0), real(b.1)).as_bytes());
                }
            }
            Ok(Built { content: c, image: None, uses_font: false })
        }
    }
}

fn add_image(doc: &mut Document, im: &RgbaImage) -> Result<ObjectId, SignError> {
    let (w, h) = im.dimensions();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let mut alpha = Vec::with_capacity((w * h) as usize);
    let mut has_alpha = false;
    for p in im.pixels() {
        rgb.extend_from_slice(&p.0[..3]);
        alpha.push(p[3]);
        has_alpha |= p[3] != 255;
    }
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Image".to_vec()));
    dict.set("Width", Object::Integer(w as i64));
    dict.set("Height", Object::Integer(h as i64));
    dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
    dict.set("BitsPerComponent", Object::Integer(8));
    if has_alpha {
        let mut sm = Dictionary::new();
        sm.set("Type", Object::Name(b"XObject".to_vec()));
        sm.set("Subtype", Object::Name(b"Image".to_vec()));
        sm.set("Width", Object::Integer(w as i64));
        sm.set("Height", Object::Integer(h as i64));
        sm.set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
        sm.set("BitsPerComponent", Object::Integer(8));
        let mut s = Stream::new(sm, alpha);
        s.compress().map_err(|e| SignError::Pdf(e.to_string()))?;
        dict.set("SMask", Object::Reference(doc.add_object(Object::Stream(s))));
    }
    let mut s = Stream::new(dict, rgb);
    s.compress().map_err(|e| SignError::Pdf(e.to_string()))?;
    Ok(doc.add_object(Object::Stream(s)))
}

fn helvetica(doc: &mut Document) -> ObjectId {
    let mut f = Dictionary::new();
    f.set("Type", Object::Name(b"Font".to_vec()));
    f.set("Subtype", Object::Name(b"Type1".to_vec()));
    f.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    f.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    doc.add_object(Object::Dictionary(f))
}

// ---------------------------------------------------------------- applying

/// Apply placements to the document. Page indices refer to `doc.get_pages()` order.
pub fn apply(doc: &mut Document, placements: &[Placement], mode: Mode) -> Result<ApplyReport, SignError> {
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut report = ApplyReport::default();
    let mut font: Option<ObjectId> = None;
    for (n, p) in placements.iter().enumerate() {
        let page = *pages.get(p.page).ok_or(SignError::PageOutOfRange(p.page))?;
        if !(p.w.is_finite() && p.h.is_finite() && p.x.is_finite() && p.y.is_finite() && p.rotation.is_finite()) || p.w < 1.0 || p.h < 1.0 {
            return Err(SignError::BadPlacement(format!("placement {} has an invalid size or position", n + 1)));
        }
        let g = page_geom(doc, page)?;
        let m = local_matrix(&g, p);
        let img_name = format!("PFImg{n}");
        let built = build_content(doc, p, &img_name, &mut report)?;
        if built.uses_font && font.is_none() {
            font = Some(helvetica(doc));
        }
        let mut res = Dictionary::new();
        if let Some(im) = built.image {
            let mut xo = Dictionary::new();
            xo.set(img_name.clone(), Object::Reference(im));
            res.set("XObject", Object::Dictionary(xo));
        }
        if built.uses_font {
            let mut fd = Dictionary::new();
            fd.set("PFHelv", Object::Reference(font.unwrap()));
            res.set("Font", Object::Dictionary(fd));
        }
        match mode {
            Mode::Annotate => annotate(doc, page, p, &m, built, res, n)?,
            Mode::Flatten => flatten(doc, page, &m, built, res)?,
        }
        report.applied += 1;
    }
    report.warnings.sort();
    report.warnings.dedup();
    Ok(report)
}

fn matrix_obj(m: &[f32; 6]) -> Object {
    Object::Array(m.iter().map(|v| Object::Real(*v)).collect())
}

fn annotate(doc: &mut Document, page: ObjectId, p: &Placement, m: &[f32; 6], b: Built, res: Dictionary, n: usize) -> Result<(), SignError> {
    let mut form = Dictionary::new();
    form.set("Type", Object::Name(b"XObject".to_vec()));
    form.set("Subtype", Object::Name(b"Form".to_vec()));
    form.set("BBox", Object::Array(vec![0.into(), 0.into(), Object::Real(p.w), Object::Real(p.h)]));
    form.set("Matrix", matrix_obj(m));
    form.set("Resources", Object::Dictionary(res));
    let mut fs = Stream::new(form, b.content);
    fs.compress().map_err(|e| SignError::Pdf(e.to_string()))?;
    let form_id = doc.add_object(Object::Stream(fs));

    let rect = bbox_in_page(m, p.w, p.h);
    let mut a = Dictionary::new();
    a.set("Type", Object::Name(b"Annot".to_vec()));
    a.set("Rect", Object::Array(rect.iter().map(|v| Object::Real(*v)).collect()));
    a.set("F", Object::Integer(4));
    a.set("NM", Object::String(format!("PDFForge-{}-{}", std::process::id(), n).into_bytes(), StringFormat::Literal));
    let mut ap = Dictionary::new();
    ap.set("N", Object::Reference(form_id));
    a.set("AP", Object::Dictionary(ap));
    match &p.content {
        Content::Image(_) => {
            a.set("Subtype", Object::Name(b"Stamp".to_vec()));
            a.set("Name", Object::Name(b"PDFForgeSignature".to_vec()));
            a.set("Contents", Object::String(b"Signature".to_vec(), StringFormat::Literal));
        }
        Content::Ink { strokes, view, width, color } => {
            a.set("Subtype", Object::Name(b"Ink".to_vec()));
            let (sx, sy) = (p.w / view.0, p.h / view.1);
            let list: Vec<Object> = strokes
                .iter()
                .filter(|s| !s.points.is_empty())
                .map(|s| {
                    Object::Array(
                        s.points
                            .iter()
                            .flat_map(|pt| {
                                let (x, y) = apply_matrix(m, pt[0] * sx, p.h - pt[1] * sy);
                                [Object::Real(x), Object::Real(y)]
                            })
                            .collect(),
                    )
                })
                .collect();
            a.set("InkList", Object::Array(list));
            a.set("C", Object::Array(color.iter().map(|v| Object::Real(*v)).collect()));
            let mut bs = Dictionary::new();
            bs.set("W", Object::Real(width * sx));
            a.set("BS", Object::Dictionary(bs));
            a.set("Contents", Object::String(b"Signature".to_vec(), StringFormat::Literal));
        }
        Content::Text { text, font_size, color } => {
            a.set("Subtype", Object::Name(b"FreeText".to_vec()));
            a.set("DA", Object::String(format!("{} rg /Helv {} Tf", rgb(*color), real(*font_size)).into_bytes(), StringFormat::Literal));
            let mut warn = false;
            let bytes = win_ansi(text, &mut warn);
            let plain: Vec<u8> = {
                // /Contents is a text string: undo the content-stream escaping
                let mut v = Vec::new();
                let mut it = bytes.iter();
                while let Some(&c) = it.next() {
                    if c == b'\\' {
                        if let Some(&n) = it.next() {
                            v.push(n);
                        }
                    } else {
                        v.push(c);
                    }
                }
                v
            };
            a.set("Contents", Object::String(plain, StringFormat::Literal));
            // Box outline off: border width 0
            let mut bs = Dictionary::new();
            bs.set("W", Object::Integer(0));
            a.set("BS", Object::Dictionary(bs));
        }
    }
    let annot_id = doc.add_object(Object::Dictionary(a));
    push_annot(doc, page, annot_id)
}

fn push_annot(doc: &mut Document, page: ObjectId, annot: ObjectId) -> Result<(), SignError> {
    let existing = match doc.objects.get(&page) {
        Some(Object::Dictionary(d)) => d.get(b"Annots").ok().cloned(),
        _ => return Err(SignError::Pdf("page object missing".into())),
    };
    match existing {
        Some(Object::Reference(r)) => {
            if let Some(Object::Array(a)) = doc.objects.get_mut(&r) {
                a.push(Object::Reference(annot));
                return Ok(());
            }
            set_annots(doc, page, vec![Object::Reference(annot)])
        }
        Some(Object::Array(mut a)) => {
            a.push(Object::Reference(annot));
            set_annots(doc, page, a)
        }
        _ => set_annots(doc, page, vec![Object::Reference(annot)]),
    }
}

fn set_annots(doc: &mut Document, page: ObjectId, a: Vec<Object>) -> Result<(), SignError> {
    if let Some(Object::Dictionary(d)) = doc.objects.get_mut(&page) {
        d.set("Annots", Object::Array(a));
        Ok(())
    } else {
        Err(SignError::Pdf("page object missing".into()))
    }
}

fn flatten(doc: &mut Document, page: ObjectId, m: &[f32; 6], b: Built, res: Dictionary) -> Result<(), SignError> {
    // Give the page its own resource dictionary (it may be inherited or shared) and merge ours in.
    let mut page_res = match inherited(doc, page, b"Resources") {
        Some(Object::Dictionary(d)) => d,
        _ => Dictionary::new(),
    };
    for (key, add) in res.iter() {
        let Object::Dictionary(add) = add else { continue };
        let mut sub = match page_res.get(key).ok().and_then(|o| dict(doc, o)) {
            Some(d) => d.clone(),
            None => Dictionary::new(),
        };
        for (k, v) in add.iter() {
            sub.set(k.clone(), v.clone());
        }
        page_res.set(key.clone(), Object::Dictionary(sub));
    }

    // Wrap the existing content in q/Q so our drawing starts from a clean graphics state.
    let existing: Vec<Object> = match doc.objects.get(&page) {
        Some(Object::Dictionary(d)) => match d.get(b"Contents") {
            Ok(Object::Reference(r)) => match doc.objects.get(r) {
                Some(Object::Array(a)) => a.clone(),
                _ => vec![Object::Reference(*r)],
            },
            Ok(Object::Array(a)) => a.clone(),
            _ => vec![],
        },
        _ => return Err(SignError::Pdf("page object missing".into())),
    };
    let _ = array; // (kept for symmetry with build.rs helpers)
    let q = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), b"q".to_vec())));
    let big_q = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), b"Q".to_vec())));
    let mut ours = format!(
        "q {} {} {} {} {} {} cm\n",
        real(m[0]), real(m[1]), real(m[2]), real(m[3]), real(m[4]), real(m[5])
    )
    .into_bytes();
    ours.extend_from_slice(&b.content);
    ours.extend_from_slice(b"\nQ");
    let mut os = Stream::new(Dictionary::new(), ours);
    os.compress().map_err(|e| SignError::Pdf(e.to_string()))?;
    let ours_id = doc.add_object(Object::Stream(os));

    let mut list = vec![Object::Reference(q)];
    list.extend(existing);
    list.push(Object::Reference(big_q));
    list.push(Object::Reference(ours_id));
    if let Some(Object::Dictionary(d)) = doc.objects.get_mut(&page) {
        d.set("Contents", Object::Array(list));
        d.set("Resources", Object::Dictionary(page_res));
    }
    Ok(())
}

/// Reserved for phase 2 (PKCS#7/CMS certified signatures).
///
/// A signed PDF stores `/ByteRange [0 a b c]` plus a hex `/Contents <...>` placeholder. The file is
/// written once with a fixed-size zero-filled placeholder, then the digest is computed over
/// everything except the placeholder, and the CMS blob is patched in. These helpers compute and
/// patch the range deterministically so a signer can be plugged in without changing the writer.
pub mod byte_range {
    /// Size in bytes of the signature container we reserve (hex digits = 2x this).
    pub const RESERVED_BYTES: usize = 16 * 1024;

    /// `[0, a, b, c]`: bytes before the `<`, and bytes after the `>` of the hex string that spans
    /// `contents_start..contents_end` (offsets of `<` and one-past `>`).
    pub fn compute(total_len: usize, contents_start: usize, contents_end: usize) -> Option<[usize; 4]> {
        if contents_start >= contents_end || contents_end > total_len {
            return None;
        }
        Some([0, contents_start, contents_end, total_len - contents_end])
    }

    /// Write a hex-encoded signature into the placeholder, zero-padded; None if it does not fit.
    pub fn patch(file: &mut [u8], contents_start: usize, contents_end: usize, signature: &[u8]) -> Option<()> {
        let hex_len = contents_end.checked_sub(contents_start)?.checked_sub(2)?; // minus '<' '>'
        if signature.len() * 2 > hex_len || file.get(contents_end - 1) != Some(&b'>') || file.get(contents_start) != Some(&b'<') {
            return None;
        }
        let mut i = contents_start + 1;
        for b in signature {
            let h = format!("{b:02X}");
            file[i..i + 2].copy_from_slice(h.as_bytes());
            i += 2;
        }
        Some(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn range_excludes_exactly_the_placeholder() {
            let file = b"AAAA<0000000000>BBBB".to_vec();
            let (s, e) = (4, 16);
            let r = compute(file.len(), s, e).unwrap();
            assert_eq!(r, [0, 4, 16, 4]);
            assert_eq!(&file[r[0]..r[1]], b"AAAA");
            assert_eq!(&file[r[2]..r[2] + r[3]], b"BBBB");
            assert!(compute(10, 8, 4).is_none() && compute(10, 2, 30).is_none());
        }
        #[test]
        fn patch_fills_and_rejects_oversize() {
            let mut file = b"AAAA<0000000000>BBBB".to_vec();
            patch(&mut file, 4, 16, &[0xAB, 0xCD]).unwrap();
            assert_eq!(&file, b"AAAA<ABCD000000>BBBB");
            assert!(patch(&mut file, 4, 16, &[0u8; 6]).is_none());
        }
    }
}
