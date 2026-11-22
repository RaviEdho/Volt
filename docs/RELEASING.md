# Releasing Volt

Pushes to `main` and pull requests run [Windows CI](../.github/workflows/ci.yml). The [release workflow](../.github/workflows/release.yml) runs when you push a tag starting with `v`.

## Cut a GitHub release

Replace `0.2.2` with the version you want to release. The version command updates the manifests and lockfiles together. You can commit the version bump with your code changes or separately; the tag must point to a commit that declares the matching version.

```sh
npm run version:set -- 0.2.2
npm run version:check

git add package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
git commit -m "Release 0.2.2"
git push origin main
git tag -a v0.2.2 -m "Volt 0.2.2"
git push origin v0.2.2
```

The workflow checks the tag, runs the tests, and attaches the Windows x64 NSIS installer and its SHA-256 checksum to a **draft** under GitHub Releases. Review the draft and click **Publish release** when ready. Publishing a release does not change the repository’s visibility.

Installers are unsigned. The workflow does not enable in-app updates.
