# Apple Silicon releases

Novelite TUI uses a native Apple Silicon executable distributed through `hirovel/homebrew-tap`. Homebrew manages installation and updates; application preferences and manuscripts are stored separately. The release archive contains the executable and license notices. Its complete corresponding source, including the Crossterm patch, is the matching version tag in this repository.

## Publish a version

1. Update the package version in `Cargo.toml` and `Cargo.lock`, review the source, and run the local checks. Commit only product changes.
2. Create the matching stable tag, such as `v0.1.0`, and push that tag. **Apple Silicon release** runs on a native M1 runner with Rust 1.97.0 and the locked dependencies.
3. The workflow checks the tag/source version, runs the existing workspace behavior tests on Apple Silicon, signs the executable ad hoc, verifies its architecture, deployment target and system-only dynamic dependencies, and installs the actual archive in a temporary Homebrew tap. Only successful package and Homebrew tests proceed to a public Release containing the archive, `SHA256SUMS`, and the generated formula.
4. With GitHub CLI signed in locally, update an existing checkout of the tap:

   ```sh
   python3 scripts/release.py update-tap --tag v0.1.0 --tap ../homebrew-tap
   ```

   This downloads and verifies the published assets, then replaces only `Formula/novelite-tui.rb`. Review, commit and push that file in the tap. Run its **Verify Apple Silicon installation** workflow to check the public installation and update commands.

The update command does not commit or push automatically and requires no additional token in either repository. A published version's archive is never replaced; corrections use a new version. A failed build leaves the current formula unchanged. A failed upload stays a draft and may be retried with the same tag through the release workflow's manual input.

## Compatibility and verification

The executable targets `aarch64-apple-darwin` with `MACOSX_DEPLOYMENT_TARGET=13.0`, a release baseline rather than a minimum imposed by Rust or the hardware. Packaging rejects a later Mach-O minimum OS or any dynamic dependency outside Apple's system library directories. Building and command-line/Homebrew checks run on macOS 15 Apple Silicon; this does not establish interactive compatibility on every system above the deployment target.

Ad-hoc signing provides the executable signature needed by Apple Silicon. It is not Apple Developer ID signing or notarization. The workflow never removes quarantine attributes or changes Gatekeeper, terminal fonts, key bindings or background settings.

The Homebrew checks cover installation, the CLI, architecture, signature and linkage. Chinese input, system clipboard behavior, mouse interaction, resizing, visual quality and screen response still require actual terminal use.
