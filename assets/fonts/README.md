# 넣어 둔 글꼴

Ghostty가 기본으로 넣어 두는 글꼴과 같은 것을 넣어, 설정이 비어 있어도 Ghostty와 같게 보이게 합니다(#13).

| 파일 | 글꼴 | 판 | 라이선스 | 출처 |
|---|---|---|---|---|
| `JetBrainsMono-{Regular,Bold,Italic,BoldItalic}.ttf` | JetBrains Mono | 2.304 | SIL OFL 1.1(`JetBrainsMono-OFL.txt`) | https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304 |
| `SymbolsNerdFontMono-Regular.ttf` | Symbols Nerd Font Mono | 3.4.0 | MIT(`SymbolsNerdFont-LICENSE.txt`) | https://github.com/ryanoasis/nerd-fonts/releases/tag/v3.4.0 (`NerdFontsSymbolsOnly.zip`) |

글꼴 파일은 고치지 않았습니다. 실행 파일에 그대로 묶여 들어갑니다(`bt-render`의 `include_bytes!`).
