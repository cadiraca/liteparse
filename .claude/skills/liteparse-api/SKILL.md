---
name: liteparse-api
description: Parse PDFs and documents into text, markdown or JSON with bounding boxes through the deployed LiteParse HTTP API (liteparse-server). Use when the user wants to extract text or layout from a PDF or image, call the LiteParse API, or test/debug the server.
---

# LiteParse HTTP API

`liteparse-server` (crate `crates/liteparse-server`) wraps the LiteParse library in an HTTP API. It is deployed on Coolify (app `liteparse-api`, context `carlab`).

## Connection

- Base URL: `http://liteparse.carlab.local` (internal network only). Locally: `http://localhost:8080`.
- Auth: send the key in `x-api-key: <key>` or `Authorization: Bearer <key>`. The key is the `API_KEY` env var of the Coolify app; read it with `coolify app env list fq82rlw1sawpng7z89fuckjl -s`. Never commit or echo it. If the server runs without `API_KEY`, `/parse` is open.

## Endpoints

### `GET /health`
No auth. Returns `{"status":"ok","version":"..."}`.

### `POST /parse`
Body is either:
- `multipart/form-data` with a `file` field, or
- the raw document bytes (any other content type).

Query parameters (all optional):

| Param | Default | Meaning |
|---|---|---|
| `format` | `json` | `json`, `text`, or `markdown` (`md`) |
| `ocr` | `true` | `false` skips OCR; much faster on digital PDFs |
| `ocr_language` | `eng` | Tesseract code. The image only ships `eng` data. |
| `ocr_server_url` | none | External OCR server (see `OCR_API_SPEC.md`) instead of Tesseract |
| `target_pages` | all | e.g. `1-5,10` |
| `max_pages` | 1000 | Page cap |
| `dpi` | library default | Render DPI for OCR |
| `password` | none | Password for encrypted PDFs |

Responses: `json` returns `application/json` (pages, text items, bounding boxes); `text` returns `text/plain`; `markdown` returns `text/markdown`.

## Examples

```bash
BASE=http://liteparse.carlab.local
KEY=...   # from Coolify, never hardcode

# Markdown, multipart upload
curl -H "x-api-key: $KEY" -F file=@doc.pdf "$BASE/parse?format=markdown"

# JSON with bounding boxes, raw body, no OCR
curl -H "x-api-key: $KEY" --data-binary @doc.pdf "$BASE/parse?ocr=false" > out.json

# Pages 2-4 only, plain text
curl -H "x-api-key: $KEY" -F file=@doc.pdf "$BASE/parse?format=text&target_pages=2-4"
```

Python:

```python
import requests
r = requests.post(f"{BASE}/parse", params={"format": "json", "ocr": "false"},
                  headers={"x-api-key": KEY}, files={"file": open("doc.pdf", "rb")})
r.raise_for_status()
pages = r.json()["pages"]
```

## Errors

Error bodies are `{"error": "<message>"}`.

| Status | Cause |
|---|---|
| 400 | Empty body, missing `file` field, or unknown `format` |
| 401 | Missing or wrong API key |
| 413 | Upload over `MAX_UPLOAD_MB` (default 50) |
| 422 | Document could not be parsed or converted, e.g. `unsupported file format` for non-PDF bytes |

## Limits and gotchas

- Formats: PDFs and images work. DOCX/XLSX/PPTX need LibreOffice, which the deployed image (`server.Dockerfile`) does not include.
- Concurrency is capped by `MAX_CONCURRENT_PARSES` (default 2); extra requests wait.
- OCR only runs on image regions and text-sparse areas, so digital PDFs parse fast. Pass `ocr=false` when you know the PDF has real text.
- Scans in non-English languages need either more `tesseract-ocr-<lang>` packages in the image or an external `ocr_server_url`.

## Verify a deployment

```bash
BASE_URL=$BASE API_KEY=$KEY scripts/smoke_test_server.sh
```

Add `RESOLVE=liteparse.carlab.local:80:127.0.0.1` when running on the Coolify host. Redeploy with `coolify deploy uuid fq82rlw1sawpng7z89fuckjl` after pushing to branch `feat/http-server`.
