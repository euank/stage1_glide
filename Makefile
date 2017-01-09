.PHONY: stage1_appc
stage1_appc:
	cargo build --release
	./scripts/package-stage1-glide.sh
