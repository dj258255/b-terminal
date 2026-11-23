# 넣어 둔 테마

[iTerm2-Color-Schemes](https://github.com/mbadolato/iTerm2-Color-Schemes)(MIT, `themes-LICENSE.txt`)의 `ghostty/` 폴더를 그대로 넣었습니다(릴리스 `release-20261116-150709-494b809`, 커밋 494b809, 914개). Ghostty가 고정한 판과 같습니다(Ghostty #15311). 판을 올릴 때는 Ghostty `build.zig.zon`의 판을 따르고, GitHub 트리의 blob sha와 `git hash-object`로 폴더 전체를 대조합니다. 파일 이름이 테마 이름입니다(`theme = Dracula`).

빌드할 때 `bt-core/build.rs`가 이 폴더를 실행 파일에 묶으므로 Ghostty를 깔지 않아도 됩니다. 사용자 테마 폴더(`~/.config/b-terminal/themes`)에 같은 이름이 있으면 그쪽이 이깁니다.
