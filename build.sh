#!/usr/bin/env bash
set -euo pipefail

# JDK 21 이상이 필요하다. JAVA_HOME이 있으면 그것을, 없으면 PATH의 javac와 jar를 쓴다.
if [[ -n "${JAVA_HOME:-}" ]]; then
    JAVAC="$JAVA_HOME/bin/javac"
    JAR="$JAVA_HOME/bin/jar"
else
    JAVAC="javac"
    JAR="jar"
fi

SNAKEYAML_VERSION="2.4"
SNAKEYAML_SHA1="e0666b825b796f85521f02360e77f4c92c5a7a07"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

LIB_DIR="$SCRIPT_DIR/lib"
BUILD_DIR="$SCRIPT_DIR/build"
CLASSES_DIR="$BUILD_DIR/classes"
DIST_DIR="$SCRIPT_DIR/dist"
SNAKEYAML_JAR="$LIB_DIR/snakeyaml-$SNAKEYAML_VERSION.jar"

sha1_of() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 1 "$1" | cut -d' ' -f1
    else
        sha1sum "$1" | cut -d' ' -f1
    fi
}

# SnakeYAML이 없으면 Maven Central에서 받고, 체크섬이 다르면 멈춘다
if [[ ! -f "$SNAKEYAML_JAR" ]]; then
    mkdir -p "$LIB_DIR"
    curl -fsSL -o "$SNAKEYAML_JAR" \
        "https://repo1.maven.org/maven2/org/yaml/snakeyaml/$SNAKEYAML_VERSION/snakeyaml-$SNAKEYAML_VERSION.jar"
    if [[ "$(sha1_of "$SNAKEYAML_JAR")" != "$SNAKEYAML_SHA1" ]]; then
        rm -f "$SNAKEYAML_JAR"
        echo "snakeyaml-$SNAKEYAML_VERSION.jar 의 SHA-1이 다르다" >&2
        exit 1
    fi
fi

rm -rf "$BUILD_DIR" "$DIST_DIR"
mkdir -p "$CLASSES_DIR" "$DIST_DIR"

# Unpack snakeyaml into classes directory
(cd "$CLASSES_DIR" && "$JAR" -xf "$SNAKEYAML_JAR")
rm -rf "$CLASSES_DIR/META-INF/MANIFEST.MF"

# Find and compile Java source files
find "$SCRIPT_DIR/src" -name "*.java" > "$BUILD_DIR/sources.txt"
"$JAVAC" -encoding UTF-8 -d "$CLASSES_DIR" -cp "$CLASSES_DIR" @"$BUILD_DIR/sources.txt"

# Build fat jar
"$JAR" --create --file "$DIST_DIR/wl-engine.jar" --main-class withlog.cli.Main -C "$CLASSES_DIR" .

echo "Build successful: $DIST_DIR/wl-engine.jar"
