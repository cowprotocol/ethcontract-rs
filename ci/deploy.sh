#!/bin/bash

set -e

token=$CARGO_TOKEN
tag=$TRAVIS_TAG
options=""
while [[ $# -gt 0 ]]; do
	case $1 in
		--token) token="$2"; shift;;
		--tag) tag="$2"; shift;;
		--dry-run) options+="--dry-run ";;
		-v|-vv|--verbose) options+="--verbose ";;
		-h|--help) cat << EOF
ci/deploy.sh
Deploy the workspace to crates.io

USAGE:
    $0 [OPTIONS]

OPTIONS:
        --token <TOKEN>             Token to use when uploading
    -t, --tag <TAG>                 The current tag being deployed
        --dry-run                   Perform all checks without uploading
    -v, --verbose                   Use verbose output (-vv very verbose/build.rs output)
    -h, --help                      Prints this help information
EOF
			;;
		*) cat << EOF
ERROR: Invalid argument '$1'.
       For more information try '$0 --help'
EOF
			;;
	esac
	shift
done

function check_manifest_version {
	version=$(cat $1 | grep '^version' | sed -n 's/version = "\(.*\)"/v\1/p')
	if [[ ! $version = $tag ]]; then
		echo "$1 is at $version but expected $tag"
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
	#   its index
	sleep 10
}

cargo_publish common
cargo_publish generate
cargo_publish derive
cargo_publish .
