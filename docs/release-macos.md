# macOS 배포: 서명과 공증

다른 Mac에서 b-terminal.app을 열 수 있으려면 Apple Developer ID로 서명하고 Apple에 공증을 받아야 합니다. 그렇지 않으면 Gatekeeper가 막습니다. 계정과 인증서는 범수 님 몫이고, 나머지는 `scripts/bundle-macos.sh`가 합니다(#88).

## 한 번만 준비

1. Apple Developer Program에 가입합니다(유료).
2. Xcode나 developer.apple.com에서 **Developer ID Application** 인증서를 만들어 키체인에 넣습니다. 이름 확인:
   ```bash
   security find-identity -v -p codesigning
   # "Developer ID Application: 이름 (TEAMID)"
   ```
3. 공증용 키체인 프로필을 만듭니다. 비밀번호는 appleid.apple.com에서 만든 앱 전용 암호입니다. 저장소에는 아무것도 남지 않습니다.
   ```bash
   xcrun notarytool store-credentials b-terminal-notary \
     --apple-id "애플 ID 메일" --team-id TEAMID --password "앱 전용 암호"
   ```

## 배포판 만들기

```bash
scripts/bundle-macos.sh --sign "Developer ID Application: 이름 (TEAMID)" --notarize b-terminal-notary --dmg
```

순서: release 빌드 → 앱 묶음 → `btctl`과 앱을 하드닝 런타임·타임스탬프로 서명(`scripts/b-terminal.entitlements`) → zip으로 공증 제출과 기다림 → 스테이플 → `spctl` 검사 → DMG 만들기·서명·공증·스테이플. 결과는 `dist/b-terminal.app`과 `dist/b-terminal-판.dmg`입니다.

환경 변수 `BT_SIGN_IDENTITY`, `BT_NOTARY_PROFILE`로도 줄 수 있습니다.

## 계정 없이

옵션 없이 돌리면 하드닝 런타임으로 **임시 서명**합니다. 이 컴퓨터에서만 뜹니다. 하드닝 런타임에서 앱과 셸이 도는지는 이 방식으로 확인했습니다(아래).

## 권한(entitlements)

지금은 비어 있습니다. b-terminal이 띄우는 셸과 그 안의 프로그램은 각자의 서명으로 돌기 때문에 앱에 따로 권한이 필요 없습니다. 더할 것이 생기면 이유와 함께 이 절에 적습니다.

## 확인한 것과 하지 않은 것

- 확인함(2026-11-06, macOS 26.3.1): 하드닝 런타임 임시 서명(`flags=0x10002(adhoc,runtime)`), `codesign --verify --strict` 통과, `open`으로 앱이 뜨고 로그인 셸(zsh)이 돌며 `btctl run`이 됨, DMG `hdiutil verify` 통과.
- 이 확인은 CPU 길이 기본이던 때(위 날짜)의 것입니다. macOS 기본이 GPU 길(wgpu, Metal)이 된 0.32 이후의 앱을 하드닝 런타임으로 서명해 띄워 본 기록은 이 저장소에 없습니다(재지 않았습니다).
- 하지 않음: Developer ID 서명, 공증 제출, 스테이플, 다른 Mac에서 열기. 계정이 없어서입니다.
