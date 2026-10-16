//! OpenLyrics import and export, Bible import and migration from OpenLP,
//! from the console.

use std::path::PathBuf;

use asaf_core::Song;
use asaf_library::openlp::{OpenLpReport, find_openlp_files};
use asaf_library::{BibleImport, ImportReport};
use asaf_i18n::catalog::fill;
use gpui::component::WindowExt as _;
use gpui::component::notification::Notification;
use gpui::{App, Context, PathPromptOptions, Window};

use crate::i18n::{Lang, lang};
use crate::{Console, SharedLibrary};

pub fn import(window: &mut Window, cx: &mut Context<Console>) {
    let library = cx.entity().read(cx).library().clone();
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: true,
        multiple: true,
        prompt: Some(lang(cx).t("Import").into()),
    });
    cx.spawn_in(window, async move |this, cx| {
        let Ok(Ok(Some(paths))) = paths.await else { return };
        let report = cx
            .background_executor()
            .spawn(async move { library.lock().unwrap().import_openlyrics(&paths) })
            .await;
        let _ = this.update_in(cx, |this, window, cx| {
            let l = lang(cx);
            let note = match report {
                Ok(r) if r.failed.is_empty() => Notification::success(import_summary(l, &r)),
                Ok(r) => {
                    let details: Vec<String> = r
                        .failed
                        .iter()
                        .take(3)
                        .map(|(p, e)| format!("{}: {e}", file_name(p)))
                        .collect();
                    Notification::warning(format!("{}\n{}", import_summary(l, &r), details.join("\n")))
                }
                Err(e) => Notification::error(l.f("Import failed: {error}", &[("error", &e.to_string())])),
            };
            window.push_notification(note, cx);
            this.refresh_search(cx);
            this.resolve_cue(cx);
        });
    })
    .detach();
}

/// Imports OSIS or Zefania Bibles and switches to the last one imported.
pub fn import_bible(window: &mut Window, cx: &mut Context<Console>) {
    let library = cx.entity().read(cx).library().clone();
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: Some(lang(cx).t("Import Bible").into()),
    });
    cx.spawn_in(window, async move |this, cx| {
        let Ok(Ok(Some(paths))) = paths.await else { return };
        let results = cx
            .background_executor()
            .spawn(async move {
                let mut library = library.lock().unwrap();
                paths.iter().map(|p| library.import_bible(p)).collect::<Vec<_>>()
            })
            .await;
        let _ = this.update_in(cx, |this, window, cx| {
            let l = lang(cx);
            let mut last = None;
            for result in results {
                let note = match result {
                    Ok(report) => {
                        last = report.bible.id;
                        Notification::success(bible_summary(l, &report))
                    }
                    Err(e) => Notification::error(l.f("Bible import failed: {error}", &[("error", &e.to_string())])),
                };
                window.push_notification(note, cx);
            }
            this.refresh_bibles(last, cx);
        });
    })
    .detach();
}

/// Asks for an OpenLP data folder and imports its songs and Bibles in the
/// background; `done` runs afterwards (refresh what shows the library).
pub fn import_openlp<V: 'static>(
    library: SharedLibrary,
    window: &mut Window,
    cx: &mut Context<V>,
    done: impl FnOnce(&mut V, &mut Context<V>) + 'static,
) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some(lang(cx).t("Import from OpenLP").into()),
    });
    cx.spawn_in(window, async move |this, cx| {
        let Ok(Ok(Some(mut dirs))) = paths.await else { return };
        let Some(dir) = dirs.pop() else { return };
        let searched = dir.clone();
        let result = cx.background_executor().spawn(async move { library.lock().unwrap().import_openlp(&dir) }).await;
        let _ = this.update_in(cx, |this, window, cx| {
            let l = lang(cx);
            let note = match result {
                Ok(report) => {
                    let problems = openlp_problems(l, &report);
                    if problems.is_empty() {
                        Notification::success(openlp_summary(l, &report))
                    } else {
                        let shown: Vec<&str> = problems.iter().take(3).map(String::as_str).collect();
                        Notification::warning(format!("{}\n{}", openlp_summary(l, &report), shown.join("\n")))
                    }
                }
                Err(_) if find_openlp_files(&searched) == Default::default() => {
                    let dir = searched.display().to_string();
                    Notification::error(l.f("No OpenLP songs.sqlite or Bible databases in {folder}.", &[("folder", &dir)]))
                }
                Err(e) => Notification::error(e),
            };
            window.push_notification(note, cx);
            done(this, cx);
        });
    })
    .detach();
}

pub fn export_all(window: &mut Window, cx: &mut Context<Console>) {
    let library = cx.entity().read(cx).library().clone();
    let dir = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some(lang(cx).t("Export Here").into()),
    });
    cx.spawn_in(window, async move |_, cx| {
        let Ok(Ok(Some(mut dirs))) = dir.await else { return };
        let Some(dir) = dirs.pop() else { return };
        let result = cx
            .background_executor()
            .spawn(async move {
                let songs = library.lock().unwrap().all().map_err(|e| e.to_string())?;
                asaf_openlyrics::write_folder(&songs, &dir)
                    .map(|written| (written.len(), dir))
                    .map_err(|e| e.to_string())
            })
            .await;
        let _ = cx.update(|window, cx| {
            let l = lang(cx);
            let note = match result {
                Ok((n, dir)) => {
                    let exported = l.n(n as u64, "Exported {count} song to {folder}", "Exported {count} songs to {folder}");
                    Notification::success(fill(&exported, &[("folder", &dir.display().to_string())]))
                }
                Err(e) => Notification::error(l.f("Export failed: {error}", &[("error", &e)])),
            };
            window.push_notification(note, cx);
        });
    })
    .detach();
}

pub fn export_song(song: Song, window: &mut Window, cx: &mut App) {
    let start = documents_dir();
    let name = format!("{}.xml", asaf_openlyrics::file_stem(&song.title));
    let path = cx.prompt_for_new_path(&start, Some(&name));
    window
        .spawn(cx, async move |cx| {
            let Ok(Ok(Some(path))) = path.await else { return };
            let result = std::fs::write(&path, asaf_openlyrics::to_xml(&song));
            let _ = cx.update(|window, cx| {
                let l = lang(cx);
                let note = match result {
                    Ok(()) => Notification::success(l.f("Exported “{title}”", &[("title", &song.title)])),
                    Err(e) => Notification::error(l.f("Export failed: {error}", &[("error", &e.to_string())])),
                };
                window.push_notification(note, cx);
            });
        })
        .detach();
}

/// `~/Documents` if it exists: where save dialogs start.
pub(crate) fn documents_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Documents"))
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// What an OpenLyrics import did, one sentence per line.
fn import_summary(l: Lang, r: &ImportReport) -> String {
    let mut lines = vec![l.n(r.imported as u64, "Imported {count} song.", "Imported {count} songs.")];
    if r.duplicates > 0 {
        lines.push(l.n(
            r.duplicates as u64,
            "Skipped {count} song already in the library.",
            "Skipped {count} songs already in the library.",
        ));
    }
    if !r.failed.is_empty() {
        lines.push(l.n(r.failed.len() as u64, "{count} file couldn't be imported.", "{count} files couldn't be imported."));
    }
    lines.join("\n")
}

/// What importing one Bible did.
fn bible_summary(l: Lang, b: &BibleImport) -> String {
    let verses = b.verses.to_string();
    let mut s = l.f(
        "Imported {name} ({abbreviation}), {verses} verses.",
        &[("name", &b.bible.name), ("abbreviation", &b.bible.abbreviation), ("verses", &verses)],
    );
    if !b.skipped_books.is_empty() {
        s.push('\n');
        s.push_str(&l.f("Skipped books outside the 66: {books}.", &[("books", &b.skipped_books.join(", "))]));
    }
    s
}

/// What an OpenLP migration did, one sentence per line.
fn openlp_summary(l: Lang, r: &OpenLpReport) -> String {
    let mut lines = vec![match &r.files.songs {
        Some(_) => import_summary(l, &r.songs),
        None => l.t("No OpenLP song database.").to_string(),
    }];
    let ok: Vec<&str> = r.bibles.iter().filter_map(|(_, b)| b.as_ref().ok()).map(|b| b.bible.abbreviation.as_str()).collect();
    let failed = r.bibles.len() - ok.len();
    lines.push(match ok.len() {
        0 => l.t("No Bibles imported.").to_string(),
        n => fill(&l.n(n as u64, "Imported {count} Bible ({names}).", "Imported {count} Bibles ({names})."), &[("names", &ok.join(", "))]),
    });
    if failed > 0 {
        lines.push(l.n(failed as u64, "{count} Bible wasn't imported.", "{count} Bibles weren't imported."));
    }
    lines.join("\n")
}

/// Every problem in an OpenLP migration, one line each.
fn openlp_problems(l: Lang, r: &OpenLpReport) -> Vec<String> {
    let mut out: Vec<String> = r.songs.failed.iter().map(|(p, e)| format!("{}: {e}", p.display())).collect();
    for (path, result) in &r.bibles {
        match result {
            Err(e) => out.push(format!("{}: {e}", file_name(path))),
            Ok(b) if !b.skipped_books.is_empty() => out.push(l.f(
                "{bible}: skipped books outside the 66: {books}",
                &[("bible", &b.bible.abbreviation), ("books", &b.skipped_books.join(", "))],
            )),
            Ok(_) => {}
        }
    }
    out
}

fn file_name(p: &std::path::Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

#[cfg(test)]
mod tests {
    use asaf_core::{Bible, Language};

    use super::*;

    #[test]
    fn reports_are_whole_sentences() {
        let l = Lang(Language::English);
        let songs = ImportReport { imported: 1, duplicates: 2, failed: vec![("a.xml".into(), "bad".into())] };
        assert_eq!(
            import_summary(l, &songs),
            "Imported 1 song.\nSkipped 2 songs already in the library.\n1 file couldn't be imported."
        );
        let bible = |abbreviation: &str| BibleImport {
            bible: Bible { name: "Reina-Valera 1909".into(), abbreviation: abbreviation.into(), ..Default::default() },
            verses: 31102,
            skipped_books: Vec::new(),
        };
        assert_eq!(bible_summary(l, &bible("RV1909")), "Imported Reina-Valera 1909 (RV1909), 31102 verses.");
        let openlp = OpenLpReport {
            files: Default::default(),
            songs: ImportReport::default(),
            bibles: vec![("a".into(), Ok(bible("RV1909"))), ("b".into(), Ok(bible("KJV"))), ("c".into(), Err("bad".into()))],
        };
        assert_eq!(
            openlp_summary(l, &openlp),
            "No OpenLP song database.\nImported 2 Bibles (RV1909, KJV).\n1 Bible wasn't imported."
        );
    }
}
