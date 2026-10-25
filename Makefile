# Small convenience entry points. Run `make help`.
.PHONY: help bundle bundle-release run-bundle licenses ffmpeg

help:
	@echo "make ffmpeg      build the LGPL FFmpeg (ffmpeg, ffprobe, libvpx, libopus) into packaging/ffmpeg/out (macOS arm64)"
	@echo "make bundle      build release binaries and assemble target/bundle/CaptureMann.app (macOS); bundles packaging/ffmpeg/out when it exists (CM_NO_FFMPEG=1 skips)"
	@echo "make bundle-release  like bundle, but fails unless the LGPL FFmpeg and its licence files are bundled"
	@echo "make run-bundle  bundle, then open the app (permissions attach to CaptureMann)"
	@echo "make licenses    regenerate THIRD_PARTY_LICENSES.md (bundled into the app)"
	@echo "env: CM_BUNDLE_OUT, CM_FFMPEG_BUNDLE, CM_NO_FFMPEG, CM_SIGN_IDENTITY, CM_BUNDLE_ID, CM_SKIP_BUILD=1 (see packaging/macos/README.md)"

licenses:
	bash packaging/macos/gen-third-party.sh

ffmpeg:
	bash packaging/ffmpeg/build-macos.sh

bundle:
	bash packaging/macos/bundle.sh

bundle-release:
	CM_REQUIRE_FFMPEG=1 bash packaging/macos/bundle.sh

run-bundle: bundle
	open target/bundle/CaptureMann.app
