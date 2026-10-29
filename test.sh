#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
./build.sh
JAVAC="${JAVA_HOME:+$JAVA_HOME/bin/}javac"
JAVA="${JAVA_HOME:+$JAVA_HOME/bin/}java"
mkdir -p build/test-classes
"$JAVAC" -encoding UTF-8 -cp dist/wl-engine.jar -d build/test-classes test/withlog/CommentCommandsTest.java test/withlog/ReindexPathsTest.java test/withlog/git/VaultGitTest.java
"$JAVA" -cp dist/wl-engine.jar:build/test-classes withlog.CommentCommandsTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes withlog.ReindexPathsTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes withlog.git.VaultGitTest
