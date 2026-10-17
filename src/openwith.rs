//! "Birlikte aç" alt menüsü: bir dosya türünü açabilen uygulamalar. Liste arka planda bir kez
//! okunur ve oturum boyunca türe göre saklanır; menü her karede diske dokunmaz.
//! Windows'ta `SHAssocEnumHandlers` (Explorer'ın kullandığı liste), Linux'ta `.desktop` dosyaları.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub struct Handler {
    /// Windows'ta programın yolu, Linux'ta `.desktop` kimliği
    pub id: String,
    pub name: String,
    pub default: bool,
    pub icon: Option<egui::TextureHandle>,
}

/// Yüklenince dolan liste
pub type Handlers = Arc<OnceLock<Vec<Handler>>>;

static CACHE: Mutex<Option<HashMap<String, Handlers>>> = Mutex::new(None);

/// Önbellek anahtarı: klasörler için tek, dosyalar için uzantı. Uzantısız dosyaların türü
/// içeriğe göre bulunduğundan onlar yola göre saklanır.
fn key(path: &Path, is_dir: bool) -> String {
    if is_dir {
        return "<klasör>".into();
    }
    match path.extension() {
        Some(e) => format!(".{}", e.to_string_lossy().to_lowercase()),
        None => path.display().to_string(),
    }
}

/// Dosyayı açabilen uygulamalar; ilk istekte arka planda yüklenir (`get()` o sırada `None`)
pub fn handlers(ctx: &egui::Context, path: &Path, is_dir: bool) -> Handlers {
    let mut cache = CACHE.lock().unwrap();
    let map = cache.get_or_insert_with(HashMap::new);
    if let Some(h) = map.get(&key(path, is_dir)) {
        return h.clone();
    }
    let slot: Handlers = Arc::new(OnceLock::new());
    map.insert(key(path, is_dir), slot.clone());
    let (path, ctx, out) = (path.to_path_buf(), ctx.clone(), slot.clone());
    let px = (16.0 * ctx.pixels_per_point()).ceil() as i32;
    std::thread::spawn(move || {
        crate::shell::init_worker_thread();
        let list = imp::list(&path, is_dir, px)
            .into_iter()
            .map(|(id, name, default, img)| {
                let icon = img.map(|img| ctx.load_texture(format!("openwith:{id}"), img, egui::TextureOptions::LINEAR));
                Handler { id, name, default, icon }
            })
            .collect();
        let _ = out.set(list);
        ctx.request_repaint();
    });
    slot
}

/// Varsayılan uygulama değişince listeler yeniden okunur
#[cfg_attr(windows, allow(dead_code))]
pub fn invalidate() {
    *CACHE.lock().unwrap() = None;
}

/// `id` uygulamasıyla dosyaları açar; açılamazsa `false`
pub fn launch(id: &str, files: &[PathBuf]) -> bool {
    // Dosya başına ayrı açılan programlar için üst sınır ("Aç"taki gibi)
    imp::launch(id, &files[..files.len().min(32)])
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn list(path: &Path, is_dir: bool, px: i32) -> Vec<(String, String, bool, Option<egui::ColorImage>)> {
        let mime = if is_dir { Some("inode/directory".to_string()) } else { crate::win::mime_of(path) };
        let Some(mime) = mime else { return Vec::new() };
        let (apps, default) = crate::desktop_apps::apps_for(&mime);
        apps.into_iter()
            // Terminal içinde çalışan programlar (Terminal=true) şimdilik listelenmez
            // Uygulamanın kendisi: klasörler için "Swift Explorer" menüde ayrıca var
            .filter(|a| !a.terminal && a.id.strip_suffix(".desktop") != Some(crate::APP_ID))
            .map(|a| {
                let icon = crate::shell::app_icon(a.icon.as_deref(), px);
                let is_default = default.as_deref() == Some(a.id.as_str());
                (a.id, a.name, is_default, icon)
            })
            .collect()
    }

    pub fn launch(id: &str, files: &[PathBuf]) -> bool {
        crate::desktop_apps::find(id).is_some_and(|a| crate::desktop_apps::launch(&a, files))
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::System::Com::{CoTaskMemFree, IDataObject};
    use windows::Win32::UI::Shell::{BHID_DataObject, IAssocHandler, IShellItem, SHAssocEnumHandlers, SHCreateItemFromParsingName, ASSOC_FILTER_RECOMMENDED};

    unsafe fn take(p: PWSTR) -> String {
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as _));
        s
    }

    /// Uzantı için önerilen uygulamalar (Explorer'ın "Birlikte aç" alt menüsündeki liste)
    fn enum_handlers(path: &Path) -> Vec<IAssocHandler> {
        let Some(ext) = path.extension() else { return Vec::new() };
        let ext = HSTRING::from(format!(".{}", ext.to_string_lossy()));
        let mut out = Vec::new();
        unsafe {
            let Ok(e) = SHAssocEnumHandlers(&ext, ASSOC_FILTER_RECOMMENDED) else { return out };
            loop {
                let mut item = [None];
                let mut fetched = 0u32;
                if e.Next(&mut item, Some(&mut fetched)).is_err() || fetched == 0 {
                    break;
                }
                out.extend(item[0].take());
                if out.len() >= 64 {
                    break;
                }
            }
        }
        out
    }

    pub fn list(path: &Path, is_dir: bool, px: i32) -> Vec<(String, String, bool, Option<egui::ColorImage>)> {
        if is_dir {
            return Vec::new();
        }
        let default = crate::win::default_app(path).and_then(|a| a.exe).map(|e| e.to_string_lossy().to_lowercase());
        let mut v: Vec<_> = enum_handlers(path)
            .into_iter()
            .filter_map(|h| unsafe {
                let id = take(h.GetName().ok()?);
                let name = take(h.GetUIName().ok()?);
                let mut icon_path = PWSTR::null();
                let mut index = 0;
                let icon = if h.GetIconLocation(&mut icon_path, &mut index).is_ok() && !icon_path.is_null() {
                    let p = take(icon_path);
                    crate::shell::shell_image(Path::new(&p), px, true)
                } else {
                    None
                };
                let icon = icon.or_else(|| crate::shell::shell_image(Path::new(&id), px, true));
                let is_default = default.as_deref() == Some(id.to_lowercase().as_str());
                Some((id, name, is_default, icon))
            })
            .collect();
        // Varsayılan uygulama başta
        v.sort_by_key(|h| !h.2);
        v
    }

    pub fn launch(id: &str, files: &[PathBuf]) -> bool {
        let mut ok = false;
        for f in files {
            unsafe {
                let Some(h) = enum_handlers(f).into_iter().find(|h| h.GetName().ok().map(|n| take(n)).is_some_and(|n| n.eq_ignore_ascii_case(id))) else { continue };
                let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(f.as_os_str()), None) else { continue };
                let Ok(data) = item.BindToHandler::<_, IDataObject>(None, &BHID_DataObject) else { continue };
                ok |= h.Invoke(&data).is_ok();
            }
        }
        ok
    }
}
