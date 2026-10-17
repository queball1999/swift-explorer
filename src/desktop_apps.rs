//! Linux: kurulu uygulamaların (`.desktop` dosyaları) listesi, bir dosya türünü açabilen
//! uygulamalar ve bir uygulamanın dosyalarla başlatılması. Varsayılan uygulamayı değiştirme
//! `xdg-mime` ile yapılır (`mimeapps.list`'e yazar).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct DesktopApp {
    /// `org.gnome.TextEditor.desktop` gibi
    pub id: String,
    pub name: String,
    pub mimes: Vec<String>,
    /// `Icon=` değeri: tema ikon adı ya da mutlak yol
    pub icon: Option<String>,
    /// `Exec=` değeri (alan kodlarıyla, kaçışları çözülmemiş)
    pub exec: String,
    /// Terminal içinde çalışması gereken (`Terminal=true`) bir program
    pub terminal: bool,
    /// `Path=`: programın çalışma klasörü
    pub workdir: Option<String>,
    /// `.desktop` dosyasının yolu (`%k` için)
    pub file: PathBuf,
}

struct Entry {
    name: String,
    icon: Option<String>,
    no_display: bool,
    mimes: Vec<String>,
    exec: String,
    try_exec: Option<String>,
    only_show_in: Vec<String>,
    not_show_in: Vec<String>,
    terminal: bool,
    workdir: Option<String>,
}

/// `LANG` değerinden aday yerel adlar: "de_DE.UTF-8" → ["de_DE", "de"]
fn locales() -> Vec<String> {
    let lang = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty())).unwrap_or_default();
    let base = lang.split(['.', '@']).next().unwrap_or("").to_string();
    let mut v = Vec::new();
    if !base.is_empty() && base != "C" && base != "POSIX" {
        if let Some((l, _)) = base.split_once('_') {
            v.push(base.clone());
            v.push(l.to_string());
        } else {
            v.push(base);
        }
    }
    v
}

fn list(v: &str) -> Vec<String> {
    v.split(';').map(str::trim).filter(|m| !m.is_empty()).map(String::from).collect()
}

/// `[Desktop Entry]` bölümünü okur; uygulama değilse ya da gizliyse `None` döner
fn parse_entry(text: &str, locales: &[String]) -> Option<Entry> {
    let (mut name, mut localized, mut app, mut hidden) = (None, None, false, false);
    let (mut no_display, mut mimes, mut icon) = (false, Vec::new(), None);
    let (mut exec, mut try_exec, mut terminal, mut workdir) = (String::new(), None, false, None);
    let (mut only_show_in, mut not_show_in) = (Vec::new(), Vec::new());
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == "[Desktop Entry]";
            continue;
        }
        if !in_section {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim();
        match k.trim() {
            "Type" => app = v == "Application",
            "Name" => name = Some(v.to_string()),
            "Icon" => icon = Some(v.to_string()).filter(|i| !i.is_empty()),
            "NoDisplay" => no_display = v == "true",
            "Hidden" => hidden = v == "true",
            "MimeType" => mimes = list(v),
            "Exec" => exec = v.to_string(),
            "TryExec" => try_exec = Some(v.to_string()).filter(|t| !t.is_empty()),
            "OnlyShowIn" => only_show_in = list(v),
            "NotShowIn" => not_show_in = list(v),
            "Terminal" => terminal = v == "true",
            "Path" => workdir = Some(v.to_string()).filter(|p| !p.is_empty()),
            k => {
                if let Some(loc) = k.strip_prefix("Name[").and_then(|r| r.strip_suffix(']')) {
                    // Daha önce eşleşen (daha belirgin) yerel ad korunur
                    if let Some(rank) = locales.iter().position(|l| l == loc) {
                        if localized.as_ref().is_none_or(|(r, _): &(usize, String)| rank < *r) {
                            localized = Some((rank, v.to_string()));
                        }
                    }
                }
            }
        }
    }
    if !app || hidden {
        return None;
    }
    Some(Entry { name: localized.map(|(_, n)| n).or(name)?, icon, no_display, mimes, exec, try_exec, only_show_in, not_show_in, terminal, workdir })
}

impl Entry {
    /// Menülerde gösterilir mi: `NoDisplay`, `OnlyShowIn`/`NotShowIn` ve `TryExec` kuralları
    fn shown(&self, desktops: &[String]) -> bool {
        if self.no_display {
            return false;
        }
        if !self.only_show_in.is_empty() && !self.only_show_in.iter().any(|d| desktops.contains(d)) {
            return false;
        }
        if self.not_show_in.iter().any(|d| desktops.contains(d)) {
            return false;
        }
        // Flatpak içinde host'un PATH'i görünmez; orada program var sayılır
        self.try_exec.as_deref().is_none_or(|t| crate::win::in_flatpak() || on_path(t))
    }

    fn into_app(self, id: String, file: PathBuf) -> DesktopApp {
        DesktopApp { id, name: self.name, mimes: self.mimes, icon: self.icon, exec: self.exec, terminal: self.terminal, workdir: self.workdir, file }
    }
}

/// `XDG_CURRENT_DESKTOP` ("GNOME", "KDE", "ubuntu:GNOME"…)
fn current_desktops() -> Vec<String> {
    std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().split(':').filter(|d| !d.is_empty()).map(String::from).collect()
}

fn on_path(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

fn data_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    if crate::win::in_flatpak() {
        // Sandbox'ın kendi XDG dizinleri yerine kullanıcının ve host'un uygulamaları
        // (`--filesystem=host` host'un /usr'ünü /run/host altında gösterir)
        v.push(home().join(".local/share"));
        v.push(PathBuf::from("/run/host/usr/local/share"));
        v.push(PathBuf::from("/run/host/usr/share"));
    }
    v.extend(dirs::data_dir());
    let system = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    v.extend(system.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    // Flatpak uygulamaları buraya dışa aktarılır
    v.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    v.push(home().join(".local/share/flatpak/exports/share"));
    dedup(v)
}

fn dedup(mut v: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = Vec::new();
    v.retain(|d| {
        let new = !seen.contains(d);
        seen.push(d.clone());
        new
    });
    v
}

/// Görünür tüm uygulamalar, ada göre sıralı. Aynı kimlik birden çok yerdeyse ilk bulunan geçerlidir
/// (kullanıcının `Hidden=true` ile gizlediği bir sistem uygulaması da gizli kalır).
pub fn installed_apps() -> Vec<DesktopApp> {
    let (locales, desktops) = (locales(), current_desktops());
    let mut seen = std::collections::HashSet::new();
    let mut apps: Vec<DesktopApp> = Vec::new();
    for dir in data_dirs() {
        let Ok(rd) = std::fs::read_dir(dir.join("applications")) else { continue };
        for de in rd.flatten() {
            let id = de.file_name().to_string_lossy().into_owned();
            if !id.ends_with(".desktop") || !seen.insert(id.clone()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(de.path()) else { continue };
            if let Some(e) = parse_entry(&text, &locales).filter(|e| e.shown(&desktops)) {
                apps.push(e.into_app(id, de.path()));
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// Kimliği verilen uygulama (NoDisplay olanlar dahil)
pub fn find(id: &str) -> Option<DesktopApp> {
    let locales = locales();
    data_dirs().into_iter().find_map(|d| {
        let file = d.join("applications").join(id);
        let text = std::fs::read_to_string(&file).ok()?;
        Some(parse_entry(&text, &locales)?.into_app(id.to_string(), file))
    })
}

/// Bir uygulamanın görünen adı (NoDisplay olanlar dahil)
pub fn app_name(id: &str) -> Option<String> {
    find(id).map(|a| a.name)
}

/// `xdg-mime default <kimlik> <tür>`: dosya türünün varsayılan uygulamasını ayarlar
pub fn set_default(id: &str, mime: &str) -> bool {
    crate::win::host_command("xdg-mime").args(["default", id, mime]).status().is_ok_and(|s| s.success())
}

// ---------------------------------------------------------------- dosya türü → uygulamalar

/// shared-mime-info `subclasses` dosyasındaki "alt üst" satırlarından bir türün doğrudan üstleri
fn parents_in(text: &str, mime: &str) -> Vec<String> {
    text.lines().filter_map(|l| l.split_once(' ')).filter(|(c, _)| *c == mime).map(|(_, p)| p.trim().to_string()).collect()
}

/// Türün kendisi ve üst türleri, yakından uzağa: `text/x-rust` → [text/x-rust, text/plain, application/octet-stream]
fn mime_lineage(mime: &str) -> Vec<String> {
    let texts: Vec<String> = data_dirs().iter().filter_map(|d| std::fs::read_to_string(d.join("mime/subclasses")).ok()).collect();
    let mut out = vec![mime.to_string()];
    let mut i = 0;
    while i < out.len() && out.len() < 16 {
        for t in &texts {
            for p in parents_in(t, &out[i]) {
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        // Spesifikasyona göre tüm metin türleri text/plain'in alt türüdür
        if out[i].starts_with("text/") && !out.iter().any(|m| m == "text/plain") {
            out.push("text/plain".into());
        }
        i += 1;
    }
    // Her şeyin üstü olan "herhangi bir dosya" türü listeyi her uygulamayla doldurur
    out.retain(|m| m != "application/octet-stream");
    out
}

/// Türün varsayılan uygulaması; tür için ayarlanmamışsa en yakın üst türünkü
/// (`text/x-rust` dosyalarını `text/plain`'in varsayılanı açar)
pub fn default_for(mime: &str) -> Option<String> {
    default_in(&mime_lineage(mime))
}

fn default_in(lineage: &[String]) -> Option<String> {
    lineage.iter().find_map(|m| crate::win::default_app_id(m))
}

/// `mimeapps.list` dosyaları, öncelik sırasıyla (mime-apps spesifikasyonu)
fn mimeapps_files() -> Vec<PathBuf> {
    let config = if crate::win::in_flatpak() { home().join(".config") } else { dirs::config_dir().unwrap_or_else(|| home().join(".config")) };
    let mut v = Vec::new();
    for d in current_desktops() {
        v.push(config.join(format!("{}-mimeapps.list", d.to_lowercase())));
    }
    v.push(config.join("mimeapps.list"));
    let sys = std::env::var("XDG_CONFIG_DIRS").unwrap_or_else(|_| "/etc/xdg".into());
    v.extend(sys.split(':').filter(|d| !d.is_empty()).map(|d| Path::new(d).join("mimeapps.list")));
    v.extend(data_dirs().into_iter().map(|d| d.join("applications/mimeapps.list")));
    dedup(v)
}

/// Bir `mimeapps.list` bölümünde türe bağlanan uygulama kimlikleri
fn section_ids(text: &str, section: &str, mime: &str) -> Vec<String> {
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == section;
        } else if in_section {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == mime {
                    return list(v);
                }
            }
        }
    }
    Vec::new()
}

/// Kullanıcının eklediği ("Birlikte aç" ile seçilen) ve kaldırdığı uygulamalar. Öncelikli dosyada
/// kaldırılan bir uygulama sonraki dosyalarda eklenmiş olsa da kaldırılmış sayılır.
fn associations(texts: &[String], mime: &str) -> (Vec<String>, Vec<String>) {
    let (mut added, mut removed) = (Vec::new(), Vec::<String>::new());
    for t in texts {
        removed.extend(section_ids(t, "[Removed Associations]", mime));
        for id in section_ids(t, "[Added Associations]", mime) {
            if !removed.contains(&id) && !added.contains(&id) {
                added.push(id);
            }
        }
    }
    (added, removed)
}

/// Dosya türünü açabilen uygulamalar: önce varsayılan, sonra kullanıcının eklediği uygulamalar,
/// sonra türü (ya da üst türlerinden birini) bildirenler.
/// İkinci değer varsayılan uygulamanın kimliği.
pub fn apps_for(mime: &str) -> (Vec<DesktopApp>, Option<String>) {
    let installed = installed_apps();
    let texts: Vec<String> = mimeapps_files().iter().filter_map(|f| std::fs::read_to_string(f).ok()).collect();
    let mut out: Vec<DesktopApp> = Vec::new();
    let lineage = mime_lineage(mime);
    let default = default_in(&lineage);
    if let Some(app) = default.as_deref().and_then(|d| installed.iter().find(|a| a.id == d).cloned().or_else(|| find(d))) {
        out.push(app);
    }
    for m in lineage {
        let (added, removed) = associations(&texts, &m);
        let declared = installed.iter().filter(|a| a.mimes.contains(&m)).map(|a| a.id.clone());
        for id in added.into_iter().chain(declared) {
            if removed.contains(&id) || out.iter().any(|a| a.id == id) {
                continue;
            }
            if let Some(a) = installed.iter().find(|a| a.id == id) {
                out.push(a.clone());
            }
        }
    }
    (out, default)
}

// ---------------------------------------------------------------- Exec ve başlatma

/// `Exec=` değerindeki bir argüman; tırnaklı argümanlarda alan kodları açılmaz
#[derive(Debug, PartialEq)]
struct Arg {
    text: String,
    quoted: bool,
}

/// Desktop Entry spesifikasyonu: önce dize kaçışları (`\s` `\n` `\t` `\r` `\\`), sonra
/// boşlukla ayrılmış, çift tırnaklanabilen argümanlar (tırnak içinde `\"` `` \` `` `\$` `\\`).
/// Kapanmayan tırnak gibi geçersiz bir değerde `None`.
fn split_exec(exec: &str) -> Option<Vec<Arg>> {
    let mut s = String::with_capacity(exec.len());
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            s.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => s.push(' '),
            Some('n') => s.push('\n'),
            Some('t') => s.push('\t'),
            Some('r') => s.push('\r'),
            Some('\\') => s.push('\\'),
            Some(o) => {
                s.push('\\');
                s.push(o);
            }
            None => s.push('\\'),
        }
    }
    let mut args = Vec::new();
    let mut chars = s.chars().peekable();
    loop {
        while chars.next_if(|c| *c == ' ' || *c == '\t').is_some() {}
        let Some(&first) = chars.peek() else { break };
        if first == '"' {
            chars.next();
            let mut text = String::new();
            loop {
                match chars.next()? {
                    '"' => break,
                    '\\' => match chars.next()? {
                        c @ ('"' | '`' | '$' | '\\') => text.push(c),
                        c => {
                            text.push('\\');
                            text.push(c);
                        }
                    },
                    c => text.push(c),
                }
            }
            args.push(Arg { text, quoted: true });
        } else {
            let mut text = String::new();
            while let Some(c) = chars.next_if(|c| *c != ' ' && *c != '\t') {
                text.push(c);
            }
            args.push(Arg { text, quoted: false });
        }
    }
    Some(args)
}

/// Uygulamayı dosyalarla başlatmak için komut satırları. `%F`/`%U` alan tek komut tüm dosyaları
/// alır; `%f`/`%u` alan program her dosya için ayrı başlatılır. Alan kodu yoksa dosya sona eklenir.
fn command_lines(app: &DesktopApp, files: &[PathBuf]) -> Option<Vec<Vec<OsString>>> {
    let args = split_exec(&app.exec)?;
    if args.is_empty() {
        return None;
    }
    let codes: Vec<&str> = args.iter().filter(|a| !a.quoted).map(|a| a.text.as_str()).collect();
    let multi = codes.iter().any(|a| *a == "%F" || *a == "%U");
    let single = codes.iter().any(|a| a.contains("%f") || a.contains("%u"));
    let groups: Vec<&[PathBuf]> = if multi || files.len() <= 1 { vec![files] } else { files.chunks(1).collect() };
    let uri = |p: &PathBuf| OsString::from(crate::win::file_uri(p));
    let mut out = Vec::new();
    for group in groups {
        let mut argv: Vec<OsString> = Vec::new();
        for a in &args {
            if a.quoted {
                argv.push(a.text.replace("%%", "%").into());
                continue;
            }
            match a.text.as_str() {
                "%F" => argv.extend(group.iter().map(|p| p.clone().into_os_string())),
                "%U" => argv.extend(group.iter().map(uri)),
                "%i" => {
                    if let Some(i) = &app.icon {
                        argv.push("--icon".into());
                        argv.push(i.into());
                    }
                }
                t => {
                    let mut arg = OsString::new();
                    let mut chars = t.chars();
                    while let Some(c) = chars.next() {
                        if c != '%' {
                            arg.push(c.encode_utf8(&mut [0; 4]));
                            continue;
                        }
                        match chars.next() {
                            Some('f' | 'F') => arg.push(group.first().map_or_else(OsString::new, |p| p.clone().into_os_string())),
                            Some('u' | 'U') => arg.push(group.first().map_or_else(OsString::new, uri)),
                            Some('c') => arg.push(&app.name),
                            Some('k') => arg.push(app.file.as_os_str()),
                            Some('%') => arg.push("%"),
                            // %i tek başına değilse ve kullanımdan kalkmış %d %D %n %N %v %m: atılır
                            _ => {}
                        }
                    }
                    // Yalnızca atılan kodlardan oluşan argüman komuttan çıkar
                    if !arg.is_empty() || t.is_empty() {
                        argv.push(arg);
                    }
                }
            }
        }
        if !multi && !single {
            argv.extend(group.iter().map(|p| p.clone().into_os_string()));
        }
        if argv.first().is_none_or(|p| p.is_empty()) {
            return None;
        }
        out.push(argv);
    }
    Some(out)
}

/// Uygulamayı dosyalarla başlatır (Flatpak içinde host'ta); bir komut bile başlarsa `true`
pub fn launch(app: &DesktopApp, files: &[PathBuf]) -> bool {
    let Some(lines) = command_lines(app, files) else { return false };
    let dir = app.workdir.as_deref().map(Path::new).filter(|d| d.is_dir());
    let mut ok = false;
    for argv in lines {
        let Some((prog, rest)) = argv.split_first() else { continue };
        let mut cmd = crate::win::host_command_in(&prog.to_string_lossy(), dir);
        ok |= cmd.args(rest).spawn().is_ok();
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = "[Desktop Entry]\nType=Application\nName=Text Editor\nName[de]=Texteditor\nName[de_DE]=Texteditor (DE)\nMimeType=text/plain;text/x-log;\nExec=gedit %U\nIcon=org.gnome.gedit\n\n[Desktop Action new]\nName=New\n";

    fn app(exec: &str) -> DesktopApp {
        DesktopApp { id: "x.desktop".into(), name: "X App".into(), mimes: vec![], icon: Some("x-icon".into()), exec: exec.into(), terminal: false, workdir: None, file: "/usr/share/applications/x.desktop".into() }
    }

    fn lines(exec: &str, files: &[&str]) -> Vec<Vec<String>> {
        let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        command_lines(&app(exec), &files).unwrap().into_iter().map(|l| l.into_iter().map(|a| a.into_string().unwrap()).collect()).collect()
    }

    #[test]
    fn parses_name_and_mimes() {
        let e = parse_entry(ENTRY, &[]).unwrap();
        assert_eq!(e.name, "Text Editor");
        assert_eq!(e.mimes, ["text/plain", "text/x-log"]);
        assert!(!e.no_display);
        assert_eq!(e.icon.as_deref(), Some("org.gnome.gedit"));
        assert_eq!(e.exec, "gedit %U");
    }

    #[test]
    fn prefers_the_most_specific_locale() {
        let e = parse_entry(ENTRY, &["de_DE".into(), "de".into()]).unwrap();
        assert_eq!(e.name, "Texteditor (DE)");
        let e = parse_entry(ENTRY, &["de".into()]).unwrap();
        assert_eq!(e.name, "Texteditor");
        let e = parse_entry(ENTRY, &["fr".into()]).unwrap();
        assert_eq!(e.name, "Text Editor");
    }

    #[test]
    fn skips_non_apps_and_hidden_entries() {
        assert!(parse_entry("[Desktop Entry]\nType=Link\nName=X\n", &[]).is_none());
        assert!(parse_entry("[Desktop Entry]\nType=Application\nName=X\nHidden=true\n", &[]).is_none());
        assert!(parse_entry("[Desktop Entry]\nType=Application\nName=X\nNoDisplay=true\n", &[]).unwrap().no_display);
    }

    #[test]
    fn show_in_rules() {
        let gnome = ["GNOME".to_string()];
        let e = |extra: &str| parse_entry(&format!("[Desktop Entry]\nType=Application\nName=X\nExec=x\n{extra}"), &[]).unwrap();
        assert!(e("").shown(&gnome));
        assert!(e("OnlyShowIn=GNOME;Unity;\n").shown(&gnome));
        assert!(!e("OnlyShowIn=KDE;\n").shown(&gnome));
        assert!(!e("NotShowIn=GNOME;\n").shown(&gnome));
        assert!(!e("NoDisplay=true\n").shown(&gnome));
        assert!(e("TryExec=sh\n").shown(&gnome));
        assert!(!e("TryExec=/nonexistent/swift-explorer-test\n").shown(&gnome));
    }

    #[test]
    fn splits_exec_with_quotes_and_escapes() {
        let texts = |s: &str| split_exec(s).unwrap().into_iter().map(|a| a.text).collect::<Vec<_>>();
        assert_eq!(texts("gedit  %U"), ["gedit", "%U"]);
        assert_eq!(texts(r#""/opt/My App/run" --flag "a \"b\" \$c""#), ["/opt/My App/run", "--flag", r#"a "b" $c"#]);
        // Dosyada "\\\\" yazılır: dize kaçışından sonra "\\", tırnak kaçışından sonra tek "\"
        assert_eq!(texts(r#"sh -c "echo \\\\""#), ["sh", "-c", r"echo \"]);
        // \s yalnızca boşluk olur; tırnaksız boşluk yine argümanları ayırır
        assert_eq!(texts(r"app\sname %f"), ["app", "name", "%f"]);
        assert_eq!(texts(r#""app\sname" %f"#), ["app name", "%f"]);
        assert!(split_exec(r#"app "unclosed"#).is_none());
        assert!(split_exec(r#"app "a\"#).is_none());
        assert!(split_exec("  ").unwrap().is_empty());
        let a = split_exec(r#"x "%f""#).unwrap();
        assert!(!a[0].quoted && a[1].quoted);
    }

    #[test]
    fn expands_field_codes() {
        assert_eq!(lines("gedit %U", &["/a b.txt", "/c.txt"]), [["gedit", "file:///a%20b.txt", "file:///c.txt"]]);
        assert_eq!(lines("vlc %F", &["/a", "/b"]), [["vlc", "/a", "/b"]]);
        // %f: her dosya için ayrı süreç
        assert_eq!(lines("eog %f", &["/a", "/b"]), [["eog", "/a"], ["eog", "/b"]]);
        assert_eq!(lines("app --file=%f", &["/a"]), [["app", "--file=/a"]]);
        assert_eq!(lines("app %i %c %k", &["/a"]), [["app", "--icon", "x-icon", "X App", "/usr/share/applications/x.desktop", "/a"]]);
        // Kullanımdan kalkmış kodlar atılır, %% yüzde olur, tırnak içinde kod açılmaz
        assert_eq!(lines("app %d %m 100%% %f", &["/a"]), [["app", "100%", "/a"]]);
        assert_eq!(lines(r#"app "%f" %f"#, &["/a"]), [["app", "%f", "/a"]]);
        // Alan kodu yoksa dosya sona eklenir
        assert_eq!(lines("app", &["/a", "/b"]), [["app", "/a"], ["app", "/b"]]);
        assert_eq!(lines("app %U", &[]), [["app"]]);
        assert!(command_lines(&app("%f"), &[]).is_none());
        assert!(command_lines(&app(""), &[]).is_none());
    }

    #[test]
    fn mime_subclasses() {
        let text = "text/x-rust text/plain\napplication/x-shellscript application/x-executable\napplication/x-shellscript text/plain\n";
        assert_eq!(parents_in(text, "application/x-shellscript"), ["application/x-executable", "text/plain"]);
        assert!(parents_in(text, "text/plain").is_empty());
    }

    #[test]
    fn added_and_removed_associations() {
        let user = "[Default Applications]\ntext/plain=a.desktop\n[Added Associations]\ntext/plain=b.desktop;c.desktop;\n[Removed Associations]\ntext/plain=d.desktop;\n".to_string();
        let system = "[Added Associations]\ntext/plain=d.desktop;e.desktop;b.desktop\nimage/png=f.desktop\n".to_string();
        let (added, removed) = associations(&[user, system], "text/plain");
        assert_eq!(added, ["b.desktop", "c.desktop", "e.desktop"]);
        assert_eq!(removed, ["d.desktop"]);
    }
}
