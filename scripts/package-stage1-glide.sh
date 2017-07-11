#!/bin/sh

set -e

# TODO, --no-cache is because docker wasn't picking up changes in
# target/release/stage1_glide and I have no clue why
docker build --no-cache -t localhost/stage1_glide -f ./scripts/Dockerfile.aci .
docker save localhost/stage1_glide > intermediate_docker.tar
docker2aci intermediate_docker.tar
rm -f intermediate_docker.tar

tmpdir=$(mktemp -d "./tmp_pkg_aci_XXXXX")
tar -C "${tmpdir}" -xzf stage1_glide-latest.aci

jq -r '.annotations |= [{"name": "coreos.com/rkt/stage1/run", "value": "/init"}, {"name": "coreos.com/rkt/stage1/gc", "value": "/gc"}, {"name": "coreos.com/rkt/stage1/enter", "value": "/enter"}]' "${tmpdir}/manifest" > "${tmpdir}/manifest.new"
mv "${tmpdir}/manifest.new" "${tmpdir}/manifest"

jq -r '.name |= "aci.euank.com/stage1_glide"' "${tmpdir}/manifest" > "${tmpdir}/manifest.new"
mv "${tmpdir}/manifest.new" "${tmpdir}/manifest"

rm -f localhost-stage1_glide-latest.aci

tar -C "${tmpdir}" -c {manifest,rootfs} | gzip > stage1_glide.aci

rm -rf "${tmpdir}"
