./stage1_glide.aci: ./scripts/package-stage1-glide.sh ./scripts/Dockerfile.aci ./target/release/stage1_glide
	cargo build --release
	./scripts/package-stage1-glide.sh

.PHONY: release
release: ./target/release/stage1_glide

RUST_SOURCE=$(wildcard *.rs)

./target/release/stage1_glide: $(RUST_SOURCE)
	cargo build --release
	./scripts/package-stage1-glide.sh acis.euank.com/stage1-glide
