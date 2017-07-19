#!/bin/sh

set -e

NAME=${1:?aci name}
FILE=${2:?output file}


docker build --no-cache -t localhost/stage1_glide -f ./scripts/Dockerfile.aci .

tmpdir=$(mktemp -d "./tmp_pkg_aci_XXXXX")

trap "rm -rf ${tmpdir}" EXIT

docker save localhost/stage1_glide > "${tmpdir}/intermediate_docker.tar"
pushd "${tmpdir}"

docker2aci intermediate_docker.tar
rm -f intermediate_docker.tar

tar xzf stage1_glide-latest.aci
rm -f glide-latest.aci

jq -r '.annotations |= [{"name": "coreos.com/rkt/stage1/run", "value": "/init"}, {"name": "coreos.com/rkt/stage1/gc", "value": "/gc"}, {"name": "coreos.com/rkt/stage1/enter", "value": "/enter"}]' manifest > manifest.new
mv manifest.new manifest

jq -r ".name |= \"${NAME}\"" manifest > manifest.new
mv manifest.new manifest

popd

tar -C "${tmpdir}" -c {manifest,rootfs} | gzip > "${FILE}"

rm -rf "${tmpdir}"
