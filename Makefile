./stage1-glide.aci: ./scripts/package-stage1-glide.sh ./scripts/Dockerfile.aci ./target/release/stage1_glide
	./scripts/package-stage1-glide.sh acis.euank.com/stage1-glide stage1-glide.aci

.PHONY: release
release: ./target/release/stage1_glide ./stage1_glide.aci

RUST_SOURCE=$(wildcard **/*.rs)

./target/release/stage1_glide: $(RUST_SOURCE) Cargo.lock Cargo.toml
	cargo build --release

./target/debug/stage1_glide: $(RUST_SOURCE) Cargo.lock Cargo.toml
	cargo build

.PHONY: debug
debug: ./stage1-glide.debug.aci

./stage1-glide.debug.aci: ./target/debug/stage1_glide ./scripts/package-stage1-glide.sh ./scripts/Dockerfile.debug.aci
	./scripts/package-stage1-glide.sh "acis.euank.com/stage1-glide" "stage1-glide.debug.aci"
