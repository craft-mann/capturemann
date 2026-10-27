# Small convenience entry points. Run `make help`.
.PHONY: report help bundle bundle-dev bundle-release run-bundle run-bundle-dev licenses report ffmpeg llama dmg notarize verify-release release

help:
	@echo "make ffmpeg      build the LGPL FFmpeg (ffmpeg, ffprobe, libvpx, libopus) into packaging/ffmpeg/out (macOS arm64)"
	@echo "make llama       build llama-server (MIT, Metal, static) into packaging/llama/out (macOS arm64); pin LLAMA_TAG and LLAMA_SHA256 in packaging/llama/build-macos.sh first"
	@echo "make bundle      build release binaries and assemble target/bundle/CaptureMann.app (macOS); bundles packaging/ffmpeg/out and packaging/llama/out when they exist (CM_NO_FFMPEG=1, CM_NO_LLAMA=1 skip)"
	@echo "make bundle-release  like bundle, but fails unless the LGPL FFmpeg, llama-server and their licence files are bundled"
	@echo "make run-bundle  bundle, then open the app (permissions attach to CaptureMann)"
	@echo "make bundle-dev  like bundle, as target/bundle/CaptureMann-dev.app with the bundle id dev.capturemann.app.dev: keeps your own builds' permission grants apart from the installed release (make run-bundle-dev opens it)"
	@echo "make report     run the tests, cargo deny and Trivy and write reports/ (commit it with a release)"
	@echo "make licenses    regenerate THIRD_PARTY_LICENSES.md (bundled into the app)"
	@echo "make dmg         build target/bundle/CaptureMann-<version>-arm64.dmg from target/bundle/CaptureMann.app (signed when CM_SIGN_IDENTITY is a real identity)"
	@echo "make notarize    submit that DMG to Apple, wait, staple (APPLE_API_KEY_ID/ISSUER/KEY_PATH or APPLE_ID/TEAM_ID/APP_PASSWORD)"
	@echo "make verify-release  check the DMG: signature, hardened runtime, entitlements, Gatekeeper, ticket, LGPL FFmpeg, version (ARGS=--allow-unsigned for a local build)"
	@echo "make release     bundle-release with CM_HARDENED=1, then dmg, notarize, verify-release (needs a Developer ID identity and notary credentials)"
	@echo "env: CM_BUNDLE_OUT, CM_FFMPEG_BUNDLE, CM_NO_FFMPEG, CM_LLAMA_BUNDLE, CM_NO_LLAMA, CM_SIGN_IDENTITY, CM_BUNDLE_ID, CM_SKIP_BUILD=1 (see packaging/macos/README.md)"

report:
	bash scripts/quality-report.sh

licenses:
	bash packaging/macos/gen-third-party.sh

ffmpeg:
	bash packaging/ffmpeg/build-macos.sh

llama:
	bash packaging/llama/build-macos.sh

bundle:
	bash packaging/macos/bundle.sh

bundle-release:
	CM_REQUIRE_FFMPEG=1 CM_REQUIRE_LLAMA=1 bash packaging/macos/bundle.sh

run-bundle: bundle
	open target/bundle/CaptureMann.app

# Development builds under their own bundle id (the TCC identity), so a release installed in
# /Applications and a build from this checkout do not fight over Screen Recording grants.
DEV_BUNDLE_ID ?= dev.capturemann.app.dev

DEV_APP_NAME ?= CaptureMann-dev

bundle-dev:
	CM_BUNDLE_ID=$(DEV_BUNDLE_ID) CM_APP_NAME=$(DEV_APP_NAME) bash packaging/macos/bundle.sh

run-bundle-dev: bundle-dev
	open target/bundle/$(DEV_APP_NAME).app

# --- Release (macOS, Apple Silicon). See packaging/macos/README.md, section "Releasing". ---
VERSION := $(shell awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f && /^version *=/{gsub(/^[^"]*"|".*$$/,""); print; exit}' Cargo.toml)
APP ?= target/bundle/CaptureMann.app
DMG ?= target/bundle/CaptureMann-$(VERSION)-arm64.dmg

dmg:
	@test -d "$(APP)" || { echo "make dmg: $(APP) not found. Run 'make bundle-release' first (or 'make release')." >&2; exit 1; }
	bash packaging/macos/dmg.sh "$(APP)" "$(DMG)"

notarize:
	@test -f "$(DMG)" || { echo "make notarize: $(DMG) not found. Run 'make dmg' first." >&2; exit 1; }
	bash packaging/macos/notarize.sh "$(DMG)"

verify-release:
	@test -e "$(DMG)" || { echo "make verify-release: $(DMG) not found. Run 'make dmg' first." >&2; exit 1; }
	bash packaging/macos/verify-release.sh $(ARGS) "$(DMG)"

release:
	@missing=""; \
	case "$${CM_SIGN_IDENTITY:-}" in ""|"-") missing="$$missing\n  CM_SIGN_IDENTITY   a \"Developer ID Application: NAME (TEAMID)\" identity (or its SHA-1) in an unlocked keychain";; esac; \
	test -d packaging/ffmpeg/out -o -n "$${CM_FFMPEG_BUNDLE:-}" || missing="$$missing\n  FFmpeg             run 'make ffmpeg' (or set CM_FFMPEG_BUNDLE to an LGPL build)"; \
	test -d packaging/llama/out -o -n "$${CM_LLAMA_BUNDLE:-}" || missing="$$missing\n  llama-server       run 'make llama' (or set CM_LLAMA_BUNDLE to a llama-server build)"; \
	bash packaging/macos/notarize.sh --check-env >/dev/null 2>&1 || missing="$$missing\n  notary credentials APPLE_API_KEY_ID + APPLE_API_ISSUER + APPLE_API_KEY_PATH, or APPLE_ID + APPLE_TEAM_ID + APPLE_APP_PASSWORD, or CM_NOTARY_PROFILE"; \
	if [ -n "$$missing" ]; then printf 'make release needs, in the environment:%b\n' "$$missing" >&2; exit 1; fi
	CM_HARDENED=1 $(MAKE) bundle-release
	$(MAKE) dmg
	$(MAKE) notarize
	$(MAKE) verify-release
	@echo "release ready: $(DMG)"
