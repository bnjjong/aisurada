#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
./build.sh
JAVAC="${JAVA_HOME:+$JAVA_HOME/bin/}javac"
JAVA="${JAVA_HOME:+$JAVA_HOME/bin/}java"
mkdir -p build/test-classes
"$JAVAC" -encoding UTF-8 -cp dist/wl-engine.jar -d build/test-classes test/aisurada/*.java test/aisurada/git/*.java
"$JAVA" -cp dist/wl-engine.jar:build/test-classes aisurada.CommentCommandsTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes aisurada.ReindexPathsTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes aisurada.AgendaFactsTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes aisurada.git.VaultGitTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes aisurada.git.VaultHistoryTest
for chapter in 24 25 26 27 28 29 30 31 32 33; do
    "$JAVA" -cp dist/wl-engine.jar:build/test-classes "aisurada.Chapter${chapter}Test"
done
"$JAVA" -cp dist/wl-engine.jar:build/test-classes aisurada.AskCommandsTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes aisurada.SessionCommandsTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes aisurada.ChatCommandsTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes aisurada.Part4ReviewTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes aisurada.Chapter25To30IntegrationTest
"$JAVA" -cp dist/wl-engine.jar:build/test-classes aisurada.Chapter31IntegrationTest
