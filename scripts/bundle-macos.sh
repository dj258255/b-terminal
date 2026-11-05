#!/bin/bash
# b-terminal.app을 만든다: release 빌드 → dist/b-terminal.app (실행 파일 b-terminal·btctl, 아이콘, Info.plist).
# 서명은 이 컴퓨터에서 띄울 수 있게 임시 서명(ad-hoc)만 한다. 배포용 서명·공증은 따로다.
# 사용: scripts/bundle-macos.sh [--install]   (--install이면 ~/Applications에 복사)
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')
cargo build --release -p bt-app -p bt-ctl

APP=dist/b-terminal.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/b-terminal target/release/btctl "$APP/Contents/MacOS/"
cp assets/icon/b-terminal.icns "$APP/Contents/Resources/"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>b-terminal</string>
  <key>CFBundleDisplayName</key><string>b-terminal</string>
  <key>CFBundleIdentifier</key><string>com.dj258255.b-terminal</string>
  <key>CFBundleExecutable</key><string>b-terminal</string>
  <key>CFBundleIconFile</key><string>b-terminal</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP" >/dev/null
echo "만들었습니다: $APP ($VERSION)"

if [ "${1:-}" = "--install" ]; then
  mkdir -p "$HOME/Applications"
  rm -rf "$HOME/Applications/b-terminal.app"
  cp -R "$APP" "$HOME/Applications/"
  echo "설치했습니다: $HOME/Applications/b-terminal.app"
fi
