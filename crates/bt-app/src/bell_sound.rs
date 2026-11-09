//! 벨 소리 파일 재생(Ghostty `bell-audio-path`·`bell-audio-volume`, #165).
//! macOS는 AppKit의 `NSSound`로 재생한다(safe 함수만 쓴다). 다른 OS는 아직 재생하지 않는다.

use std::path::Path;

/// 소리 파일 하나와 그것을 읽은 `NSSound`. 재생하는 동안 살아 있어야 해서 앱이 들고 있다
#[cfg(target_os = "macos")]
pub struct BellSound {
    path: std::path::PathBuf,
    sound: objc2::rc::Retained<objc2_app_kit::NSSound>,
}

#[cfg(target_os = "macos")]
impl BellSound {
    /// 파일을 읽는다. 없거나 소리 파일이 아니면 `None`(벨은 조용히 지나간다)
    pub fn load(path: &Path) -> Option<Self> {
        use objc2::AnyThread;
        let name = objc2_foundation::NSString::from_str(path.to_str()?);
        // 참조로 쓰지 않고 읽어 둔다: 재생 도중 파일이 바뀌어도 안전하다
        let sound =
            objc2_app_kit::NSSound::initWithContentsOfFile_byReference(objc2_app_kit::NSSound::alloc(), &name, false)?;
        Some(Self {
            path: path.to_path_buf(),
            sound,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_playing(&self) -> bool {
        self.sound.isPlaying()
    }

    /// `volume`(0.0~1.0)으로 처음부터 재생한다
    pub fn play(&self, volume: f32) {
        self.sound.setVolume(volume.clamp(0.0, 1.0));
        self.sound.play();
    }
}

/// macOS가 아닌 곳은 소리 파일을 재생하지 않는다(키는 받는다)
#[cfg(not(target_os = "macos"))]
pub struct BellSound;

#[cfg(not(target_os = "macos"))]
impl BellSound {
    pub fn load(_path: &Path) -> Option<Self> {
        None
    }

    pub fn path(&self) -> &Path {
        Path::new("")
    }

    pub fn is_playing(&self) -> bool {
        false
    }

    pub fn play(&self, _volume: f32) {}
}
