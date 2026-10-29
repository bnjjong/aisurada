#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
./build.sh
JAVAC="${JAVA_HOME:+$JAVA_HOME/bin/}javac"
JAVA="${JAVA_HOME:+$JAVA_HOME/bin/}java"
mkdir -p build/test-classes
"$JAVAC" -encoding UTF-8 -cp dist/wl-engine.jar -d build/test-classes test/withlog/CommentCommandsTest.java
"$JAVA" -cp dist/wl-engine.jar:build/test-classes withlog.CommentCommandsTest
