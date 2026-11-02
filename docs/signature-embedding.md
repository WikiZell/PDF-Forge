# Signature embedding format

PDF Forge writes *visual* signatures and fill-in fields. Cryptographic (certified) signatures are
phase 2 and are **not implemented**; this document describes what is written today and how the
cryptographic layer will plug in.

Implementation: `crates/pdf-core/src/sign.rs`. Verified by rendering results with PDFium
(`crates/pdf-core/tests/signing.rs`).

## Coordinates

Placements are specified in **view space**: PDF points (1/72 in), origin at the top-left of the page
*as displayed* (after `/Rotate`), y growing downwards, and clockwise rotation in degrees about the
box centre. `sign::local_matrix` converts to PDF user space, honouring the visible box
(`/CropBox` if present, else `/MediaBox`, with inheritance) and `/Rotate` (0/90/180/270).

## Annotate mode (default, "editable")

One annotation per placement is appended to the page's `/Annots`. Every annotation has `/Rect`
(axis-aligned bounds of the rotated box in page space), `/F 4` (print) and an appearance stream
`/AP << /N form >>`.

| Source | `/Subtype` | Notes |
|---|---|---|
| Typed or imported signature (raster) | `/Stamp`, `/Name /PDFForgeSignature` | Appearance draws an image XObject (RGB, Flate) with an `/SMask` for transparency. |
| Drawn signature (vector) | `/Ink` | `/InkList` holds the strokes in page space, `/C` colour, `/BS /W` width. The appearance draws each segment with a round-capped line whose width follows recorded pen pressure. |
| Date / Text | `/FreeText` | `/Contents` holds the text, `/DA` default appearance; the appearance uses base-14 Helvetica with WinAnsi encoding (characters outside WinAnsi become `?`, reported as a warning). |

The appearance form XObject has `/BBox [0 0 w h]` (placement size in points) and a `/Matrix`
mapping that local box to page space, including page rotation and placement rotation. Because
`/Rect` is exactly the bounding box of the transformed `/BBox`, the viewer's fit step
(ISO 32000-1 §12.5.5, algorithm 8.1) is the identity, so Acrobat, Chrome and PDFium place it
identically.

## Flatten mode

Nothing is added to `/Annots`. The page content becomes
`q <original streams> Q  q a b c d e f cm <drawing> Q`, where the first `q`/`Q` pair isolates the
original graphics state. Needed resources (image XObject, `PFHelv` font) are added to a *private*
copy of the page's `/Resources`, so pages sharing an inherited resource dictionary are not
affected.

## Original file

Signing always writes a new file (temp file + fsync + rename, existing target backed up as
`<name>.bak`). The source is never opened for writing. Encrypted sources require the password and
the output is **unencrypted** (a warning is shown).

## Phase 2: certified signatures (design only)

`sign::byte_range` reserves the placeholder logic:

1. Write the document once with a signature dictionary
   `/Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached`,
   `/ByteRange [0 0000000000 0000000000 0000000000]` (fixed-width digits) and
   `/Contents <000…0>` of `RESERVED_BYTES` (16 KiB) zero bytes, appended as an **incremental
   update** so the original bytes stay intact.
2. `byte_range::compute(total_len, contents_start, contents_end)` returns the four numbers that
   exclude exactly the `<…>` placeholder; patch them into the fixed-width `/ByteRange`.
3. Hash the two covered ranges and produce a detached CMS/PKCS#7 `SignedData` (certificate chain,
   signing-time and ESS attributes) with a signer trait implemented outside `pdf-core`
   (candidates: `cms` + `x509-cert` + `rsa`/`p256`, all permissive — to be confirmed before adding).
4. `byte_range::patch` hex-encodes the CMS blob into the placeholder (zero padded); it refuses a
   blob that does not fit, in which case the caller retries with a larger reservation.

Visual appearance of a certified signature reuses the Annotate-mode `/Widget` appearance above.
No certificate handling, private-key storage or trust evaluation exists in phase 1.
