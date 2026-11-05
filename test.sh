#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
./build.sh
JAVAC="${JAVA_HOME:+$JAVA_HOME/bin/}javac"
JAVA="${JAVA_HOME:+$JAVA_HOME/bin/}java"
mkdir -p build/test-classes
"$JAVAC" -encoding UTF-8 -cp dist/ais-engine.jar -d build/test-classes test/aisurada/*.java test/aisurada/git/*.java test/aisurada/cli/*.java
"$JAVA" -cp dist/ais-engine.jar:build/test-classes aisurada.CommentCommandsTest
"$JAVA" -cp dist/ais-engine.jar:build/test-classes aisurada.ReindexPathsTest
"$JAVA" -cp dist/ais-engine.jar:build/test-classes aisurada.AgendaFactsTest
"$JAVA" -cp dist/ais-sidecar-noclaude.jar:build/test-classes aisurada.git.VaultGitTest
"$JAVA" -cp dist/ais-engine.jar:build/test-classes aisurada.git.VaultHistoryTest
for chapter in 24 25 26 27 28 29 30 31 33; do
    "$JAVA" -cp dist/ais-engine.jar:build/test-classes "aisurada.Chapter${chapter}Test"
done
"$JAVA" -Daisurada.enable.part4=true -cp dist/ais-sidecar-noclaude.jar:build/test-classes aisurada.Chapter32Test
"$JAVA" -cp dist/ais-engine.jar:build/test-classes aisurada.AskCommandsTest
"$JAVA" -cp dist/ais-engine.jar:build/test-classes aisurada.SessionCommandsTest
"$JAVA" -cp dist/ais-engine.jar:build/test-classes aisurada.ChatCommandsTest
"$JAVA" -cp dist/ais-engine.jar:build/test-classes aisurada.Part4ReviewTest
"$JAVA" -cp dist/ais-engine.jar:build/test-classes aisurada.Chapter25To30IntegrationTest
"$JAVA" -cp dist/ais-engine.jar:build/test-classes aisurada.Chapter31IntegrationTest
"$JAVA" -cp dist/ais-engine.jar:build/test-classes aisurada.cli.McpServerTest
