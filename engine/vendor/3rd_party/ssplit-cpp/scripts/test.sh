#!/bin/bash

set -eo pipefail;

if [ -z ${BINARY_DIR+UNSET} ]; then
	echo "Usage: BINARY_DIR=<> $0"
	echo "BINARY_DIR is not set."
	exit 1
fi

function evaluate-diff {
    LANGUAGE=$1
    NAME=$2
    MODE=$3
    FSUFFIX=$4
    ADDITIONAL_ARGS=$5

    echo "Testing $NAME mode of operation";

    BASE_ARGS=(
        -m${MODE} -n 
        ${ADDITIONAL_ARGS[@]} 
    )

    if [ -f nonbreaking_prefixes/nonbreaking_prefix.$LANGUAGE ]; then
        BASE_ARGS+=(
            -p nonbreaking_prefixes/nonbreaking_prefix.${LANGUAGE} 
        )
    fi

    INPUT_FILE="tests/sample.${LANGUAGE}${FSUFFIX}"
    EXPECTED_OUTPUT="tests/sample.${LANGUAGE}${FSUFFIX}.m${MODE}.n.expected"

    diff -qa <(${BINARY_DIR}/ssplit ${BASE_ARGS[@]} ${INPUT_FILE}) ${EXPECTED_OUTPUT} || (echo " - [FAIL] mapped ${NAME} mode " && return 1); 
    echo " - [SUCCESS] mapped ${NAME} mode";

    diff -qa <(${BINARY_DIR}/ssplit ${BASE_ARGS[@]} < ${INPUT_FILE}) ${EXPECTED_OUTPUT} || (echo " - [FAIL] streamed ${NAME} mode " && return 1); 
    echo " - [SUCCESS] streamed ${NAME} mode";

}

echo "File based loads"
ADDITIONAL_ARGS=""
evaluate-diff "en" "paragraph" "p" "" ${ADDITIONAL_ARGS}
evaluate-diff "en" "sentence" "s" "" ${ADDITIONAL_ARGS}
evaluate-diff "en" "wrapped" "w" ".wrapped" ${ADDITIONAL_ARGS}

echo "ByteArray based loads"
ADDITIONAL_ARGS="--byte-array=1"
evaluate-diff "en" "paragraph" "p" "" ${ADDITIONAL_ARGS}
evaluate-diff "en" "sentence" "s" "" ${ADDITIONAL_ARGS}
evaluate-diff "en" "wrapped" "w" ".wrapped" ${ADDITIONAL_ARGS}

echo "Armenian"
ADDITIONAL_ARGS=""
evaluate-diff "hy" "paragraph" "p" "" ${ADDITIONAL_ARGS}

echo "Chinese"
ADDITIONAL_ARGS=""
evaluate-diff "zh" "paragraph" "p" "" ${ADDITIONAL_ARGS}

echo "Japanese"
ADDITIONAL_ARGS=""
evaluate-diff "jp" "paragraph" "p" "" ${ADDITIONAL_ARGS}
