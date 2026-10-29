#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
./build.sh
JAVAC="${JAVA_HOME:+$JAVA_HOME/bin/}javac"
JAVA="${JAVA_HOME:+$JAVA_HOME/bin/}java"
mkdir -p build/test-classes
"$JAVAC" -encoding UTF-8 -cp dist/wl-engine.jar -d build/test-classes test/withlog/*.java test/withlog/git/*.java
"$JAVA" -cp dist/wl-engine.jar:build/test-classes withlog.CommentCommandsTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes withlog.ReindexPathsTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes withlog.AgendaFactsTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes withlog.git.VaultGitTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes withlog.git.VaultHistoryTest
for chapter in 24 25 26 27 28 29 30 31; do
    "$JAVA" -cp dist/wl-engine.jar:build/test-classes "withlog.Chapter${chapter}Test"
done
"$JAVA" -cp dist/wl-engine.jar:build/test-classes withlog.Chapter25To30IntegrationTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes withlog.Chapter31IntegrationTest
