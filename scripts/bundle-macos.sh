#!/bin/bash
# b-terminal.app을 만든다: release 빌드 → dist/b-terminal.app (실행 파일 b-terminal·btctl, 아이콘, Info.plist).
#
# 사용: scripts/bundle-macos.sh [--install] [--sign "Developer ID Application: 이름 (TEAMID)"] [--notarize 프로필] [--dmg]
#   --install       ~/Applications에 복사
#   --sign 이름     Developer ID로 서명(하드닝 런타임, 타임스탬프). 없으면 하드닝 런타임으로 임시 서명(이 컴퓨터용)
#   --notarize 프로필  notarytool 키체인 프로필로 공증하고 스테이플(--sign 필요). 프로필 만들기는 docs/release-macos.md
#   --dmg           dist/b-terminal-판.dmg도 만든다(서명·공증이 있으면 DMG도)
# 환경 변수 BT_SIGN_IDENTITY·BT_NOTARY_PROFILE로도 줄 수 있다. 비밀 값은 저장소에 두지 않는다(#88)
set -euo pipefail
cd "$(dirname "$0")/.."

INSTALL=0
DMG=0
IDENTITY="${BT_SIGN_IDENTITY:-}"
PROFILE="${BT_NOTARY_PROFILE:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    --install) INSTALL=1 ;;
    --dmg) DMG=1 ;;
    --sign) IDENTITY="${2:?--sign 뒤에 서명 이름이 필요합니다}"; shift ;;
    --notarize) PROFILE="${2:?--notarize 뒤에 프로필 이름이 필요합니다}"; shift ;;
    *) echo "모르는 인자: $1" >&2; exit 2 ;;
  esac
  shift
done
if [ -n "$PROFILE" ] && [ -z "$IDENTITY" ]; then
  echo "공증(--notarize)에는 Developer ID 서명(--sign)이 필요합니다" >&2
  exit 2
fi

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
  <!-- Dock 아이콘에 끌어놓기와 Finder의 "다음으로 열기"(#279). Ghostty의 Info.plist와 같은 세 유형: 스크립트, 폴더, 실행 파일 -->
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>command</string>
        <string>tool</string>
        <string>sh</string>
        <string>zsh</string>
        <string>csh</string>
        <string>pl</string>
      </array>
      <key>CFBundleTypeIconFile</key><string>b-terminal.icns</string>
      <key>CFBundleTypeName</key><string>Terminal scripts</string>
      <key>CFBundleTypeRole</key><string>Editor</string>
    </dict>
    <dict>
      <key>CFBundleTypeName</key><string>Folders</string>
      <key>CFBundleTypeRole</key><string>Editor</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>public.directory</string>
      </array>
    </dict>
    <dict>
      <key>CFBundleTypeRole</key><string>Shell</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>public.unix-executable</string>
      </array>
    </dict>
  </array>
</dict>
</plist>
PLIST

ENT=scripts/b-terminal.entitlements
if [ -n "$IDENTITY" ]; then
  # 안쪽 실행 파일부터, 하드닝 런타임과 타임스탬프로(공증 조건)
  SIGN=(codesign --force --options runtime --timestamp --entitlements "$ENT" --sign "$IDENTITY")
else
  # 계정 없이: 하드닝 런타임 임시 서명(이 컴퓨터에서만 뜬다, 타임스탬프 없음)
  SIGN=(codesign --force --options runtime --entitlements "$ENT" --sign -)
fi
"${SIGN[@]}" "$APP/Contents/MacOS/btctl" >/dev/null
"${SIGN[@]}" "$APP" >/dev/null
codesign --verify --strict --verbose=2 "$APP" 2>&1 | tail -1
echo "만들었습니다: $APP ($VERSION, 서명: ${IDENTITY:-임시})"

if [ -n "$PROFILE" ]; then
  ZIP=dist/b-terminal-$VERSION.zip
  ditto -c -k --keepParent "$APP" "$ZIP"
  echo "공증에 냅니다(몇 분 걸릴 수 있습니다): $ZIP"
  xcrun notarytool submit "$ZIP" --keychain-profile "$PROFILE" --wait
  xcrun stapler staple "$APP"
  spctl --assess --type execute --verbose=2 "$APP"
  rm -f "$ZIP"
fi

if [ "$DMG" = 1 ]; then
  OUT=dist/b-terminal-$VERSION.dmg
  rm -f "$OUT"
  hdiutil create -volname b-terminal -srcfolder "$APP" -ov -format UDZO "$OUT" >/dev/null
  if [ -n "$IDENTITY" ]; then
    codesign --force --timestamp --sign "$IDENTITY" "$OUT"
  fi
  if [ -n "$PROFILE" ]; then
    xcrun notarytool submit "$OUT" --keychain-profile "$PROFILE" --wait
    xcrun stapler staple "$OUT"
  fi
  echo "DMG: $OUT"
fi

if [ "$INSTALL" = 1 ]; then
  mkdir -p "$HOME/Applications"
  rm -rf "$HOME/Applications/b-terminal.app"
  cp -R "$APP" "$HOME/Applications/"
  echo "설치했습니다: $HOME/Applications/b-terminal.app"
fi
