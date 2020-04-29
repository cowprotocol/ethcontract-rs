#!/bin/bash

set -e

token=$CARGO_TOKEN
tag=$TRAVIS_TAG
options=""
while [[ $# -gt 0 ]]; do
	case $1 in
		--token) token="$2"; shift;;
		--tag) tag="$2"; shift;;
		--dry-run) options+="$1 ";;
		-v|-vv|--verbose) options+="$1 ";;
		-h|--help) cat << EOF
ci/deploy.sh
Deploy the workspace to crates.io

USAGE:
    $0 [OPTIONS]

OPTIONS:
        --token <TOKEN>     Token to use when uploading
    -t, --tag <TAG>         The current tag being deployed
        --dry-run           Perform all checks without uploading
    -v, --verbose           Use verbose output (-vv very verbose output)
    -h, --help              Prints this help information
EOF
			exit
			;;
		*) >&2 cat << EOF
ERROR: Invalid option '$1'.
       For more information try '$0 --help'
EOF
			exit 1
			;;
	esac
	shift
done

function check_manifest_version {
	version=$(cat $1 | grep '^version' | sed -n 's/version = "\(.*\)"/v\1/p')
	if [[ ! $version = $tag ]]; then
		>&2 echo "ERROR: $1 is at $version but expected $tag"
		exit 1
	fi
}

check_manifest_version common/Cargo.toml
check_manifest_version generate/Cargo.toml
check_manifest_version derive/Cargo.toml
check_manifest_version Cargo.toml

function cargo_publish {
	(cd $1; cargo publish --token $CARGO_TOKEN $options)

	# NOTE(nlordell): For some reason, the next publish fails on not being able
	#   to find the new version; maybe it takes a second for crates.io to update
	#   its index. To make the deployment script more robust wait until the
	#   crate is available on `crates.io` by polling its download link.
	if [[ $1 != "." ]]; then
		retries=15
		url="https://crates.io/api/v1/crates/ethcontract-$(basename $1)/${tag/#v/}/download"
		until [[ $retries -le 0 ]] || curl -Ifs "$url" > /dev/null; do
			retries=$(($retries - 1))
			sleep 10
		done
	fi
}

cargo_publish common
cargo_publish generate
cargo_publish derive
cargo_publish .
