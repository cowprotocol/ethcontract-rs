#!/bin/bash

set -e

verbose=""
version=""
while [[ $# -gt 0 ]]; do
	case $1 in
		-v|--verbose) verbose=t;;
		-h|--help) cat << EOF
ci/bump.sh
Bump the version of all crates and packages in the repository to VERSION

USAGE:
    $0 [OPTIONS] <VERSION>

OPTIONS:
    -v, --verbose           Use verbose output
    -h, --help              Prints this help information

ARGUMENTS:
    VERSION                 The new version to use for the workspace. It must
                            be a valid SemVer version number of the format
                            'MAJOR.MINOR.PATCH'.
EOF
			exit
			;;
		*)
			if [[ -z "$version" ]]; then
				version="$1"
			else
				>&2 cat << EOF
ERROR: Invalid option '$1'.
	   For more information try '$0 --help'
EOF
				exit 1
			fi
			;;
	esac
	shift
done

if [[ -z "$version" ]]; then
	>&2 cat << EOF
ERROR: Missing version argument.
       For more information try '$0 --help'
EOF
	exit 1
fi
if [[ ! $version =~ ^v?[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
	>&2 cat << EOF
ERROR: Invalid version format.
       For more information try '$0 --help'
EOF
	exit 1
fi
version=${version/v/}

function msg {
	if [[ -n $verbose ]]; then
		echo $*
	fi
}

msg "Updating Cargo manifests with new version '$version':"
for manifest in ethcontract*/Cargo.toml; do
	msg "  - $manifest"
	sed -i -E -e 's/^((ethcontract-[a-z]+ = \{ )?version) = "[0-9\.]+"/\1 = "'"$version"'"/g' "$manifest"
done
