# Releasing

1. `npm ci && npm run licenses && git diff --exit-code THIRD_PARTY_LICENSES`
2. Bump the version in `Cargo.toml` (`[workspace.package]`), `package.json` and
   `src-tauri/tauri.conf.json`.
3. Tag and push; the `windows` CI job produces `*.msi` and `*-setup.exe` (NSIS) as workflow
   artifacts. They are **unsigned** until step 4 is done.
4. **Authenticode (disabled until a certificate is available).** Add the certificate to the CI
   runner (e.g. import a PFX from a secret into the certificate store) and set in
   `src-tauri/tauri.conf.json`:
   ```json
   "bundle": { "windows": { "certificateThumbprint": "<thumbprint>", "digestAlgorithm": "sha256",
                            "timestampUrl": "<RFC 3161 server>" } }
   ```
   or a `signCommand`. Note: a timestamp server is a network call made *by the build*, never by the
   shipped app.
5. Smoke-test the installers on a clean Windows 10 and 11 VM with [MANUAL-QA.md](MANUAL-QA.md).

Known installer gap: Tauri's MSI (WiX) installs per-machine; only the NSIS installer is per-user.
