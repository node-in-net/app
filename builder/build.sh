#!/bin/bash
set -e

# ANSI colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Print helper functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Workspace and SCRIPT_DIR resolution
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKDIR="$(cd "$SCRIPT_DIR/.." && pwd)"

show_usage() {
    echo "Usage: $0 <platform> [app] [--publish]"
    echo "Supported platforms:"
    echo "  deb   - Debian Package (.deb)"
    echo "  rpm   - Fedora Package (.rpm)"
    echo "  zst   - Arch Linux Package (.pkg.tar.zst)"
    echo "  exe   - Windows Installer (.exe) via Wine/MSYS2 cross-compilation"
    echo "  dmg   - macOS Disk Image (.dmg)"
    echo "  apk   - Android Application Package (.apk)"
    echo "Optional arguments:"
    echo "  [app]       - Application target name (default: gtk-app)"
    echo "  --publish   - Publish build outputs to OTA update servers"
    exit 1
}

# Parse parameters
if [ -z "$1" ]; then
    show_usage
fi

PLATFORM="$1"
shift

APP="gtk-app"
PUBLISH=false

while [ "$#" -gt 0 ]; do
    case "$1" in
        --publish)
            PUBLISH=true
            shift
            ;;
        *)
            APP="$1"
            shift
            ;;
    esac
done

# Determine Cargo package name and directory based on APP
case "$APP" in
    gtk-app)
        CARGO_PKG="nodeinnet-gtk"
        APP_DIR="src/gtk-app"
        DESKTOP_FILE="com.nodeinnet.gtk.desktop"
        ;;
    *)
        CARGO_PKG="$APP"
        APP_DIR="src/$APP"
        DESKTOP_FILE="$CARGO_PKG.desktop"
        ;;
esac

# Target output directory (defaults to "distr" under WORKDIR)
OUT_DIR="${OUT_DIR:-distr}"
if [[ "$OUT_DIR" != /* ]]; then
    OUT_DIR="$WORKDIR/$OUT_DIR"
fi

# Initialize Build Environment
cd "$WORKDIR"

# Ensure local bin paths are included
export PATH="$HOME/.cargo/bin:$PATH"

# Load Version
if [ -f "$WORKDIR/package.json" ]; then
    VERSION=$(node -p "require('$WORKDIR/package.json').version")
else
    log_error "package.json not found in $WORKDIR"
    exit 1
fi

# Load Env file. The .env now lives ONE LEVEL ABOVE the app repo (next to
# build-nodeinnet.sh) — read directly for local runs (dmg). For docker runs that
# parent dir isn't mounted, so compose injects the vars via `env_file` and the
# file search below simply finds nothing (the vars are already in the environment).
ENV_FILE="$WORKDIR/../.env"
[ -f "$ENV_FILE" ] || ENV_FILE="$WORKDIR/.env"                 # fallback: inside app/
[ -f "$ENV_FILE" ] || ENV_FILE="/home/builder/workdir/.env"   # fallback: container mount

if [ -f "$ENV_FILE" ]; then
    set -a
    source "$ENV_FILE"
    set +a
    log_info "Loaded environment variables from $ENV_FILE"
fi

# Map human-readable app to gen-version.js expected app_type
case "$APP" in
    gtk-app|gui)
        GEN_APP_TYPE="gui"
        ;;
    android-app|mobile|apk)
        GEN_APP_TYPE="mobile"
        ;;
    console-app|console)
        GEN_APP_TYPE="console"
        ;;
    *)
        GEN_APP_TYPE="$APP"
        ;;
esac

log_info "----------------------------------------"
log_info "Starting compilation: Platform=$PLATFORM, App=$APP (Type: $GEN_APP_TYPE)"
log_info "Version: v$VERSION, Publish: $PUBLISH"
if command -v rustc &> /dev/null; then log_info "Rust version: $(rustc --version)"; fi
if command -v node &> /dev/null; then log_info "Node.js version: $(node --version)"; fi
log_info "----------------------------------------"

# Utility Helpers
calculate_md5() {
    local file_path="$1"
    if [ ! -f "$file_path" ]; then
        log_error "File not found: $file_path"
        return 1
    fi
    
    if command -v md5sum &> /dev/null; then
        md5sum "$file_path" | awk '{print $1}'
    elif command -v md5 &> /dev/null; then
        md5 -q "$file_path"
    elif command -v openssl &> /dev/null; then
        openssl md5 "$file_path" | awk '{print $2}'
    else
        log_error "No MD5 utility found (md5sum, md5, openssl)."
        return 1
    fi
}

register_md5() {
    local platform_label="$1"
    local file_path="$2"
    if [ -f "$file_path" ]; then
        local checksum
        checksum=$(calculate_md5 "$file_path")
        mkdir -p "$OUT_DIR"
        echo "Installer $platform_label md5: $checksum" >> "$OUT_DIR/md5sums.txt"
        log_success "Registered MD5 for $platform_label installer: $checksum"
    else
        log_error "Could not find file to register MD5: $file_path"
    fi
}


# Main Switch
case "$PLATFORM" in
    deb)
        export CARGO_TARGET_DIR="$WORKDIR/bin/distr/deb/target"
        export CARGO_HOME="$WORKDIR/bin/distr/cargo-home-shared"
        
        node ./builder/gen-version.js "$GEN_APP_TYPE" deb
        
        log_info "Building node-network..."
        cargo build -p node-network --release
        log_info "Building $CARGO_PKG..."
        cargo build -p "$CARGO_PKG" --release
        
        mkdir -p "./bin/$APP/release/"
        mkdir -p ./bin/node-network/release/
        cp "$CARGO_TARGET_DIR/release/$CARGO_PKG" "./bin/$APP/release/$CARGO_PKG"
        cp "$CARGO_TARGET_DIR/release/libnode_network.so" ./bin/node-network/release/libnode_network.so
        
        log_info "Creating Debian package..."
        cargo deb -p "$CARGO_PKG"
        
        mkdir -p "$OUT_DIR"
        cp "$CARGO_TARGET_DIR/debian"/*.deb "$OUT_DIR/"
        
        OUTPUT_FILE=$(ls -t "$OUT_DIR"/*.deb 2>/dev/null | head -n 1)
        if [ -n "$OUTPUT_FILE" ]; then
            register_md5 "Debian" "$OUTPUT_FILE"
        else
            log_error "No .deb output package found in $OUT_DIR!"
            exit 1
        fi
        ;;
        
    rpm)
        export CARGO_TARGET_DIR="$WORKDIR/bin/distr/rpm/target"
        export CARGO_HOME="$WORKDIR/bin/distr/cargo-home-shared"
        
        node ./builder/gen-version.js "$GEN_APP_TYPE" rpm
        
        log_info "Building node-network..."
        cargo build -p node-network --release
        log_info "Building $CARGO_PKG..."
        cargo build -p "$CARGO_PKG" --release
        
        mkdir -p "./bin/$APP/release/"
        mkdir -p ./bin/node-network/release/
        cp "$CARGO_TARGET_DIR/release/$CARGO_PKG" "./bin/$APP/release/$CARGO_PKG"
        cp "$CARGO_TARGET_DIR/release/libnode_network.so" ./bin/node-network/release/libnode_network.so
        
        log_info "Creating RPM package..."
        rm -rf "$CARGO_TARGET_DIR/generate-rpm"
        cd "$APP_DIR"
        cargo generate-rpm
        cd "$WORKDIR"
        
        mkdir -p "$OUT_DIR"
        cp "$CARGO_TARGET_DIR"/generate-rpm/*.rpm "$OUT_DIR/"
        
        OUTPUT_FILE=$(ls -t "$OUT_DIR"/*.rpm 2>/dev/null | head -n 1)
        if [ -n "$OUTPUT_FILE" ]; then
            register_md5 "Fedora" "$OUTPUT_FILE"
        else
            log_error "No .rpm output package found in $OUT_DIR!"
            exit 1
        fi
        ;;
        
    zst)
        export CARGO_TARGET_DIR="$WORKDIR/bin/distr/zst/target"
        export CARGO_HOME="$WORKDIR/bin/distr/cargo-home-shared"
        
        node ./builder/gen-version.js "$GEN_APP_TYPE" zst
        
        log_info "Building node-network..."
        cargo build -p node-network --release
        log_info "Building $CARGO_PKG..."
        cargo build -p "$CARGO_PKG" --release
        
        log_info "Setting makepkg options..."
        cd "$APP_DIR"
        echo "OPTIONS+=(!strip !lto !debug)" > ~/.makepkg.conf
        
        log_info "Generating PKGBUILD..."
        mkdir -p /tmp/nodeinnet-arch-build
        cat <<EOF > /tmp/nodeinnet-arch-build/PKGBUILD
pkgname=$CARGO_PKG
pkgver=$VERSION
pkgrel=1
pkgdesc="Node In Net $APP"
arch=('x86_64')
url="https://node.in.net"
license=('MIT')
depends=('gtk4' 'libadwaita')
source=()
sha256sums=()

package() {
    install -Dm755 "/home/builder/workdir/bin/distr/zst/target/release/$CARGO_PKG" "\$pkgdir/usr/bin/$CARGO_PKG"
    install -Dm755 "/home/builder/workdir/bin/distr/zst/target/release/libnode_network.so" "\$pkgdir/usr/lib/nodeinnet/libnode_network.so"
    if [ -f "/home/builder/workdir/$APP_DIR/assets/$DESKTOP_FILE" ]; then
        install -Dm644 "/home/builder/workdir/$APP_DIR/assets/$DESKTOP_FILE" "\$pkgdir/usr/share/applications/$DESKTOP_FILE"
    elif [ -f "/home/builder/workdir/src/gtk-app/assets/com.nodeinnet.gtk.desktop" ]; then
        install -Dm644 "/home/builder/workdir/src/gtk-app/assets/com.nodeinnet.gtk.desktop" "\$pkgdir/usr/share/applications/com.nodeinnet.gtk.desktop"
    fi
    if [ -f "/home/builder/workdir/$APP_DIR/assets/icon-512.png" ]; then
        install -Dm644 "/home/builder/workdir/$APP_DIR/assets/icon-512.png" "\$pkgdir/usr/share/icons/hicolor/512x512/apps/${DESKTOP_FILE%.desktop}.png"
    elif [ -f "/home/builder/workdir/src/gtk-app/assets/icon-512.png" ]; then
        install -Dm644 "/home/builder/workdir/src/gtk-app/assets/icon-512.png" "\$pkgdir/usr/share/icons/hicolor/512x512/apps/${DESKTOP_FILE%.desktop}.png"
    fi
}
EOF
        log_info "Compiling Arch Linux Package..."
        cd /tmp/nodeinnet-arch-build
        PKGDEST="$OUT_DIR/" CARGO_TARGET_DIR="/home/builder/workdir/bin/distr/zst/target" makepkg -cf
        
        cd "$WORKDIR"
        rm -rf /tmp/nodeinnet-arch-build
        
        OUTPUT_FILE=$(ls -t "$OUT_DIR"/*.pkg.tar.zst 2>/dev/null | head -n 1)
        if [ -n "$OUTPUT_FILE" ]; then
            register_md5 "Arch Linux" "$OUTPUT_FILE"
        else
            log_error "No .pkg.tar.zst output package found in $OUT_DIR!"
            exit 1
        fi
        ;;
        
    exe)
        export CARGO_TARGET_DIR="$WORKDIR/bin/distr/exe/target"
        export CARGO_HOME="$WORKDIR/bin/distr/cargo-home-shared"
        
        node ./builder/gen-version.js "$GEN_APP_TYPE" exe
        
        log_info "Cross-compiling windows target (network)..."
        cargo build --release \
            --manifest-path "$WORKDIR/src/node-network/Cargo.toml" \
            --target x86_64-pc-windows-gnu
            
        log_info "Cross-compiling windows target (app)..."
        cargo build --release \
            --manifest-path "$WORKDIR/$APP_DIR/Cargo.toml" \
            --target x86_64-pc-windows-gnu
            
        log_info "Compiling 64-bit Windows Installer with NSIS natively..."
        mkdir -p "$WORKDIR/distr"
        makensis "$WORKDIR/$APP_DIR/setup.nsi"
        
        OUTPUT_FILE="$WORKDIR/distr/$CARGO_PKG-${VERSION}-1-win64.exe"
        if [ ! -f "$OUTPUT_FILE" ]; then
            OUTPUT_FILE=$(ls -t "$WORKDIR/distr"/*.exe 2>/dev/null | head -n 1)
        fi
        
        # Move output to custom OUT_DIR if defined
        if [ "$OUT_DIR" != "$WORKDIR/distr" ] && [ -f "$OUTPUT_FILE" ]; then
            mkdir -p "$OUT_DIR"
            cp "$OUTPUT_FILE" "$OUT_DIR/"
            rm -f "$OUTPUT_FILE"
            OUTPUT_FILE="$OUT_DIR/$(basename "$OUTPUT_FILE")"
        fi
        
        if [ -f "$OUTPUT_FILE" ]; then
            register_md5 "Windows" "$OUTPUT_FILE"
        else
            log_error "Windows installer target not found at $OUTPUT_FILE"
            exit 1
        fi
        ;;
        
    dmg)
        node ./builder/gen-version.js "$GEN_APP_TYPE" dmg
        rm -f "./bin/$APP/release/$CARGO_PKG"
        rm -f ./bin/distr/gtkapp-darwin/*.dmg
        
        log_info "Building macOS GTK app bundle for $CARGO_PKG..."
        
        log_info "Building node-network..."
        cargo build -p node-network --release
        
        log_info "Running cargo bundle for $CARGO_PKG..."
        cd "$APP_DIR"
        CARGO_TARGET_DIR=../../bin/distr/gtkapp-darwin cargo bundle --release
        cd "$WORKDIR"
        
        APP_BUNDLE_PATH=$(ls -d bin/distr/gtkapp-darwin/release/bundle/osx/*.app 2>/dev/null | head -n 1)
        if [ -z "$APP_BUNDLE_PATH" ] || [ ! -d "$APP_BUNDLE_PATH" ]; then
            log_error "Failed to find generated macOS .app bundle!"
            exit 1
        fi
        
        log_info "Detected macOS App bundle: $APP_BUNDLE_PATH"
        
        mkdir -p "$APP_BUNDLE_PATH/Contents/Resources/share/glib-2.0/schemas"
        if [ -d "/opt/homebrew/share/glib-2.0/schemas" ]; then
            cp /opt/homebrew/share/glib-2.0/schemas/org.gtk.gtk4.Settings*.xml "$APP_BUNDLE_PATH/Contents/Resources/share/glib-2.0/schemas/"
        elif [ -d "/usr/local/share/glib-2.0/schemas" ]; then
            cp /usr/local/share/glib-2.0/schemas/org.gtk.gtk4.Settings*.xml "$APP_BUNDLE_PATH/Contents/Resources/share/glib-2.0/schemas/"
        fi
        
        if command -v glib-compile-schemas &> /dev/null; then
            glib-compile-schemas "$APP_BUNDLE_PATH/Contents/Resources/share/glib-2.0/schemas"
        fi
        
        mkdir -p "$APP_BUNDLE_PATH/Contents/Libs"
        DYLIB=""
        for candidate in \
            "bin/target/release/libnode_network.dylib" \
            "bin/distr/gtkapp-darwin/release/libnode_network.dylib" \
            "target/release/libnode_network.dylib"; do
            if [ -f "$candidate" ]; then DYLIB="$candidate"; break; fi
        done
        if [ -z "$DYLIB" ]; then
            log_error "libnode_network.dylib not found; the bundle would ship without it"
            exit 1
        fi
        cp "$DYLIB" "$APP_BUNDLE_PATH/Contents/Libs/libnode_network.dylib"
        
        if command -v dylibbundler &> /dev/null; then
            log_info "Running dylibbundler to package GTK dependencies..."
            dylibbundler -od -b -x "$APP_BUNDLE_PATH/Contents/MacOS/$CARGO_PKG" \
                -d "$APP_BUNDLE_PATH/Contents/Libs/" \
                -p @executable_path/../Libs/ \
                -s /Library/Developer/CommandLineTools/usr/lib/swift-5.5/macosx \
                -s /Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx \
                -s /Library/Developer/CommandLineTools/usr/lib/swift-5.0/macosx \
                -s /Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.0/macosx \
                -s /usr/lib/swift || log_warning "dylibbundler failed, continuing"
        else
            log_warning "dylibbundler not found. Skipping library bundling."
        fi
        
        log_info "Codesigning application bundle..."
        codesign --force --deep --sign - "$APP_BUNDLE_PATH" || log_warning "Codesigning failed, continuing"
        
        log_info "Packaging into DMG..."
        rm -rf ./bin/distr/dmg_stage && mkdir -p ./bin/distr/dmg_stage
        cp -r "$APP_BUNDLE_PATH" ./bin/distr/dmg_stage/
        
        DMG_NAME="$CARGO_PKG-${VERSION}-1-mac.dmg"
        mkdir -p "$OUT_DIR"
        rm -f "$OUT_DIR/$DMG_NAME"
        
        if command -v create-dmg &> /dev/null; then
            create-dmg --volname "NodeInNet $APP Installer" \
                --window-pos 200 120 \
                --window-size 800 400 \
                --icon-size 100 \
                --app-drop-link 600 185 \
                "$OUT_DIR/$DMG_NAME" \
                "./bin/distr/dmg_stage/"
        else
            log_warning "create-dmg not found. Copying .app folder only."
            tar -czf "$OUT_DIR/$CARGO_PKG-${VERSION}-1-mac.app.tar.gz" -C ./bin/distr/dmg_stage .
            DMG_NAME="$CARGO_PKG-${VERSION}-1-mac.app.tar.gz"
        fi
        
        rm -rf ./bin/distr/dmg_stage
        
        OUTPUT_FILE="$OUT_DIR/$DMG_NAME"
        if [ -f "$OUTPUT_FILE" ]; then
            register_md5 "macOS" "$OUTPUT_FILE"
        else
            log_error "macOS DMG output not found at $OUTPUT_FILE"
            exit 1
        fi
        ;;
        
    apk)
        export ANDROID_HOME=/opt/android-sdk
        export JAVA_HOME=/opt/android-studio/jbr
        export PATH=$JAVA_HOME/bin:$PATH
        export GRADLE_USER_HOME="$WORKDIR/bin/distr/gradle-home-shared"
        
        log_info "Java version: $(java --version | head -n 1)"
        if command -v javac &> /dev/null; then
            log_info "Java compiler: $(javac -version 2>&1)"
        else
            log_error "Java compiler (javac) not found. Cannot proceed."
            exit 1
        fi
        
        node ./builder/gen-version.js "$GEN_APP_TYPE" apk

        log_info "Building native library..."
        npm run build-android-node
        
        log_info "Building Android APK with Gradle..."
        cd ./src/android-app
        chmod +x ./gradlew
        
        mkdir -p ~/.android
        if [ -f "./debug.keystore" ]; then
            cp "./debug.keystore" ~/.android/debug.keystore
            log_info "Restored debug.keystore from project directory"
        fi
        
        ./gradlew assembleRelease assembleDebug
        
        if [ -f ~/.android/debug.keystore ]; then
            cp ~/.android/debug.keystore "./debug.keystore"
            log_info "Saved debug.keystore to project directory"
        fi
        
        cd "$WORKDIR"
        mkdir -p "$OUT_DIR"
        
        REL_APK_PATH="./src/android-app/app/build/outputs/apk/release/app-release-unsigned.apk"
        if [ -f "$REL_APK_PATH" ]; then
            cp "$REL_APK_PATH" "$OUT_DIR/nodeinnet-v${VERSION}-android-release-unsigned.apk"
            log_success "Release APK created: $OUT_DIR/nodeinnet-v${VERSION}-android-release-unsigned.apk"
            # Unsigned APKs are usually not published automatically, but let's check
            register_md5 "Android Release (Unsigned)" "$OUT_DIR/nodeinnet-v${VERSION}-android-release-unsigned.apk"
        else
            log_warning "Release APK not found at $REL_APK_PATH"
        fi
        
        DBG_APK_PATH="./src/android-app/app/build/outputs/apk/debug/app-debug.apk"
        if [ -f "$DBG_APK_PATH" ]; then
            cp "$DBG_APK_PATH" "$OUT_DIR/nodeinnet-v${VERSION}-android-debug.apk"
            log_success "Debug APK created: $OUT_DIR/nodeinnet-v${VERSION}-android-debug.apk"
            register_md5 "Android Debug" "$OUT_DIR/nodeinnet-v${VERSION}-android-debug.apk"
        else
            log_warning "Debug APK not found at $DBG_APK_PATH"
        fi
        ;;
        
    *)
        log_error "Unknown platform: $PLATFORM"
        show_usage
        ;;
esac

log_success "Build completed successfully for platform: $PLATFORM"
exit 0
