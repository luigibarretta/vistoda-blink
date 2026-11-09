# auth-modules provenance

`lib-auth-modules` is private, while this repository and its GitHub workflows
are public, so the engine consumes it as a vendored path dependency inside the
add-on's Docker build context. The copy is an exact `git archive` of tag
`v0.2.0`, the revision recorded in `dependencies/auth-modules.sha`
(`2a9aec766b0edb4cf0a55f8a4acd246b73b8a078`). It is licensed
`MIT OR Apache-2.0`. The engine enables only `keys` (HKDF sub-key for the
provider store; adds `hmac`, already in the RustCrypto tree) and
`secret-files` (no dependency).

`dependencies/vendored.json` declares this copy. The `auth-modules-vendor`
step (`check-vendored-dep`) verifies the tree digest in
`dependencies/auth-modules-vendor.sha256` everywhere and, when a clone exists
at `../lib-auth-modules`, compares every file with the pinned commit. CI never
needs to clone the private repository.

To refresh, from the repository root:

```bash
v=addon/vistoda_blink_engine/vendor/auth-modules
rm -rf "$v" && mkdir "$v"
git -C ../lib-auth-modules archive <commit> | tar -x -C "$v"
git -C ../lib-auth-modules rev-parse <commit> > dependencies/auth-modules.sha
node third_party/repo-tooling/check-vendored-dep.mjs --only auth-modules --update-digest
node third_party/repo-tooling/check-vendored-dep.mjs --only auth-modules --require-upstream
```
