# Bulk-signing rule file (version 1)

A rule describes *which signature to place, on which pages, and where*, so the same placement can
be applied to many PDFs. Rule files are plain JSON (UTF-8, at most 1 MiB), saved/loaded from the
Bulk sign screen. Unknown fields are rejected, and every value is validated before use
(`src-tauri/src/bulk.rs`, tests in the same file).

```json
{
  "version": 1,
  "signature": "3f9a1c0b77e2",
  "width": 150,
  "height": 50,
  "pages": "last",
  "position": {
    "type": "anchor",
    "text": "Signature:",
    "matchCase": false,
    "occurrence": "last",
    "side": "right",
    "offsetX": 8,
    "offsetY": -10
  },
  "stamp": { "type": "date", "format": "iso", "fontSize": 11, "offsetX": 0, "offsetY": 54 },
  "flatten": false
}
```

| Field | Type | Meaning |
|---|---|---|
| `version` | `1` | Required. Other values are rejected. |
| `signature` | string | Id of a signature in the local library. A rule is only portable between computers that have the same library entry. |
| `width`, `height` | number | Signature box size in points, 4–2000. |
| `pages` | `"first"` \| `"last"` \| `"all"` \| integer ≥ 1 | Which pages receive a signature. A page number beyond the document's length *skips* that file. |
| `position.type` | `"fixed"` \| `"anchor"` | |
| `position` (fixed) | `x`, `y` | Top-left corner of the box in points from the page's top-left (view space, y down). |
| `position` (anchor) | `text` (1–200 chars), `matchCase` (default false), `occurrence` (`"first"`\|`"last"`, default first), `side` (`"right"`\|`"below"`\|`"same"`, default right), `offsetX`, `offsetY` (default 0) | The text is located with PDFium's text search on each selected page. The box's top-left is the anchor's right edge / bottom-left / top-left (per `side`) plus the offsets. With `pages: "all"` each page uses its own match, and pages without one are left unsigned. If no selected page has a match the file is **skipped** ("anchor text … not found"). |
| `stamp` | `null` \| object | Optional extra text next to each signature. |
| `stamp.type` | `"date"` \| `"text"` | |
| `stamp` (date) | `format` (`"iso"` 2026-11-02, `"dmy"` 02/11/2026, `"mdy"` 11/02/2026), `fontSize` (4–200), `offsetX`, `offsetY` | The date is the computer's local date at the moment the job starts. |
| `stamp` (text) | `text` (1–200 chars), `fontSize`, `offsetX`, `offsetY` | |
| `flatten` | boolean | `false` = annotations; `true` = baked into page content. |

Offsets of a stamp are measured from the signature box's top-left corner.

## Outputs

* Written to the chosen folder as `<original name>_signed.pdf`. If that name exists, ` (2)`, ` (3)`, …
  is appended: **existing files are never overwritten** and the originals are never modified.
  Each file is written atomically (temp file + rename) after its name has been reserved with
  create-new semantics, so parallel workers cannot collide.
* Per-file outcome: **signed** (n placed), **skipped** (a reason: password protected, anchor not
  found, page does not exist, over the size/page limits, cancelled) or **failed** (damaged file,
  write error).
* A `signing-report.csv` (columns `input,status,output,signatures_placed,reason`, RFC 4180 quoting,
  leading `= + - @` neutralised against spreadsheet formula injection) is written next to the outputs.
* Workers default to the number of logical CPUs minus one (minimum 1); this is the portable
  approximation of "physical cores minus one" available without extra dependencies.
