//! OpenLyrics import and export, and Bible import, from the console.

use std::path::PathBuf;

use asaf_core::Song;
use gpui::component::WindowExt as _;
use gpui::component::notification::Notification;
use gpui::{App, Context, PathPromptOptions, Window};

use crate::Console;

pub fn import(window: &mut Window, cx: &mut Context<Console>) {
    let library = cx.entity().read(cx).library().clone();
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: true,
        multiple: true,
        prompt: Some("Import".into()),
    });
    cx.spawn_in(window, async move |this, cx| {
        let Ok(Ok(Some(paths))) = paths.await else { return };
        let report = cx
            .background_executor()
            .spawn(async move { library.lock().unwrap().import_openlyrics(&paths) })
            .await;
        let _ = this.update_in(cx, |this, window, cx| {
            let note = match report {
                Ok(r) if r.failed.is_empty() => Notification::success(r.summary()),
                Ok(r) => {
                    let details: Vec<String> = r
                        .failed
                        .iter()
                        .take(3)
                        .map(|(p, e)| format!("{}: {e}", file_name(p)))
                        .collect();
                    Notification::warning(format!("{}\n{}", r.summary(), details.join("\n")))
                }
                Err(e) => Notification::error(format!("Import failed: {e}")),
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
        prompt: Some("Import Bible".into()),
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
            let mut last = None;
            for result in results {
                let note = match result {
                    Ok(report) => {
                        last = report.bible.id;
                        Notification::success(report.summary())
                    }
                    Err(e) => Notification::error(format!("Bible import failed: {e}")),
                };
                window.push_notification(note, cx);
            }
            this.refresh_bibles(last, cx);
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
        prompt: Some("Export Here".into()),
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
            let note = match result {
                Ok((n, dir)) => Notification::success(format!("Exported {n} songs to {}", dir.display())),
                Err(e) => Notification::error(format!("Export failed: {e}")),
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
                let note = match result {
                    Ok(()) => Notification::success(format!("Exported \"{}\"", song.title)),
                    Err(e) => Notification::error(format!("Export failed: {e}")),
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

fn file_name(p: &std::path::Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}
