//! The console's language: a string table from English to Spanish. Code
//! writes the English text (`lang(cx).t("Songs")`); a test checks every such
//! string has a Spanish entry. Messages built with `format!` from other
//! parts (reference errors, import reports) stay English.

use std::collections::HashMap;
use std::sync::LazyLock;

use asaf_core::Language;
use gpui::{App, Global};

/// The language the console shows, app-wide.
pub struct UiLanguage(pub Language);

impl Global for UiLanguage {}

/// Translates for one language; cheap to copy into render code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lang(pub Language);

pub fn lang(cx: &App) -> Lang {
    Lang(cx.try_global::<UiLanguage>().map(|l| l.0).unwrap_or_default())
}

/// Switches the console's language and redraws every window.
pub fn set_language(language: Language, cx: &mut App) {
    cx.set_global(UiLanguage(language));
    cx.refresh_windows();
}

/// Marks English text that is translated later, through a variable
/// (`lang.t(label)`), so the table test can find it.
pub const fn en(text: &'static str) -> &'static str {
    text
}

static SPANISH_BY_ENGLISH: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| SPANISH.iter().copied().collect());

impl Lang {
    /// `en` in this language (itself if there's no translation).
    pub fn t(self, en: &'static str) -> &'static str {
        match self.0 {
            Language::English => en,
            Language::Spanish => SPANISH_BY_ENGLISH.get(en).copied().unwrap_or(en),
        }
    }
}

/// English → Spanish.
const SPANISH: &[(&str, &str)] = &[
    ("Search title, lyrics or author", "Buscar título, letra o autor"),
    ("Restored from the last session:", "Restaurado de la sesión anterior:"),
    ("Songs", "Canciones"),
    ("Songs (⌘1)", "Canciones (⌘1)"),
    ("Bible", "Biblia"),
    ("Bible (⌘2)", "Biblia (⌘2)"),
    ("New", "Nuevo"),
    ("New song (⌘N)", "Nueva canción (⌘N)"),
    ("Import…", "Importar…"),
    ("Import OpenLyrics files or folders", "Importar archivos o carpetas OpenLyrics"),
    ("Export…", "Exportar…"),
    ("Export every song as OpenLyrics into a folder", "Exportar todas las canciones como OpenLyrics a una carpeta"),
    ("song", "canción"),
    ("songs", "canciones"),
    ("⏎ go live, ⇧⏎ add to service", "⏎ proyectar, ⇧⏎ añadir al culto"),
    ("No songs found.", "No se encontraron canciones."),
    ("Select a song.", "Elige una canción."),
    ("PREVIEW · LIBRARY", "VISTA PREVIA · BIBLIOTECA"),
    ("Edit", "Editar"),
    ("Edit song (⌘E)", "Editar canción (⌘E)"),
    ("Add", "Añadir"),
    ("Add to the service (⇧⏎ in search)", "Añadir al culto (⇧⏎ en la búsqueda)"),
    ("Go live", "Proyectar"),
    ("Show the first slide (⏎ in search)", "Mostrar la primera diapositiva (⏎ en la búsqueda)"),
    ("This song has no slides. Check its verse order.", "Esta canción no tiene diapositivas. Revisa el orden de estrofas."),
    ("Showing text", "Mostrando texto"),
    ("Blanked to black (B)", "Pantalla en negro (B)"),
    ("Text hidden, background only (⇧B)", "Texto oculto, solo fondo (⇧B)"),
    ("Showing the logo (L)", "Mostrando el logo (L)"),
    ("Item", "Elemento"),
    ("of", "de"),
    ("Not in the service", "Fuera del culto"),
    ("● LIVE", "● EN VIVO"),
    ("Nothing live yet.", "Aún no se proyecta nada."),
    ("Theme", "Tema"),
    ("Projector display", "Pantalla del proyector"),
    ("→ ↓ Space next  ·  ← ↑ previous  ·  ⏎ cued item live", "→ ↓ Espacio siguiente  ·  ← ↑ anterior  ·  ⏎ proyectar el elemento preparado"),
    ("B black  ·  ⇧B hide text  ·  L logo  ·  Esc show text", "B negro  ·  ⇧B ocultar texto  ·  L logo  ·  Esc mostrar texto"),
    ("⌘1 songs  ·  ⌘2 Bible  ·  ⌘E edit  ·  ⌘I notes  ·  ⌘S save", "⌘1 canciones  ·  ⌘2 Biblia  ·  ⌘E editar  ·  ⌘I notas  ·  ⌘S guardar"),
    ("Themes…", "Temas…"),
    ("Edit themes (⌘T)", "Editar temas (⌘T)"),
    ("Settings…", "Ajustes…"),
    ("Settings (⌘,)", "Ajustes (⌘,)"),
    ("◀  Previous", "◀  Anterior"),
    ("Next  ▶", "Siguiente  ▶"),
    ("Hide text", "Ocultar texto"),
    ("Logo", "Logo"),
    ("Blank", "Negro"),
    ("Notes for the operator", "Notas para el operador"),
    ("Default theme", "Tema predeterminado"),
    ("Song", "Canción"),
    ("New service (⇧⌘N)", "Nuevo culto (⇧⌘N)"),
    ("Open…", "Abrir…"),
    ("Open a service (⌘O)", "Abrir un culto (⌘O)"),
    ("Save", "Guardar"),
    ("Save (⌘S), Save As (⇧⌘S)", "Guardar (⌘S), Guardar como (⇧⌘S)"),
    ("The service is empty.", "El culto está vacío."),
    ("Choose a song or type a reference on the left and press ⇧⏎ to add it, or drag songs here.", "Elige una canción o escribe una cita a la izquierda y pulsa ⇧⏎ para añadirla, o arrastra canciones aquí."),
    ("Notes (⌘I)", "Notas (⌘I)"),
    ("⌘↑↓ cue  ·  ⏎ go live  ·  ⌥↑↓ move  ·  ⌘⌫ remove", "⌘↑↓ preparar  ·  ⏎ proyectar  ·  ⌥↑↓ mover  ·  ⌘⌫ quitar"),
    ("There's no theme with that name; the default is used.", "No hay un tema con ese nombre; se usa el predeterminado."),
    ("Nothing cued. Add songs and passages to the service.", "Nada preparado. Añade canciones y pasajes al culto."),
    ("PREVIEW", "VISTA PREVIA"),
    ("Show the first slide and cue the next item (⏎)", "Mostrar la primera diapositiva y preparar el siguiente elemento (⏎)"),
    ("This item has no slides.", "Este elemento no tiene diapositivas."),
    ("Import an OSIS or Zefania Bible", "Importar una Biblia OSIS o Zefania"),
    ("Words without numbers search the text.", "Palabras sin números buscan en el texto."),
    ("⏎ show, ⏎ again go live  ·  ⌘⏎ go live  ·  ⇧⏎ add to service  ·  Esc back", "⏎ mostrar, ⏎ otra vez proyectar  ·  ⌘⏎ proyectar  ·  ⇧⏎ añadir al culto  ·  Esc volver"),
    ("Type a reference and press ⏎.", "Escribe una cita y pulsa ⏎."),
    ("PREVIEW · BIBLE", "VISTA PREVIA · BIBLIA"),
    ("Add to the service (⇧⏎)", "Añadir al culto (⇧⏎)"),
    ("Show the first slide (⏎ again, or ⌘⏎)", "Mostrar la primera diapositiva (⏎ otra vez, o ⌘⏎)"),
    ("Verses per slide", "Versículos por diapositiva"),
    ("Fit", "Ajustar"),
    ("Verse numbers", "Números de versículo"),
    ("Reference on last slide", "Cita en la última diapositiva"),
    ("Name", "Nombre"),
    ("Font", "Fuente"),
    ("Smallest text (% of height)", "Texto más pequeño (% de la altura)"),
    ("Largest text (% of height)", "Texto más grande (% de la altura)"),
    ("Line spacing", "Interlineado"),
    ("Text color", "Color del texto"),
    ("Outline color", "Color del contorno"),
    ("Outline width", "Grosor del contorno"),
    ("Shadow color", "Color de la sombra"),
    ("Shadow blur", "Difuminado de la sombra"),
    ("Shadow offset", "Desplazamiento de la sombra"),
    ("Side margins (%)", "Márgenes laterales (%)"),
    ("Top and bottom margins (%)", "Márgenes superior e inferior (%)"),
    ("Background color", "Color de fondo"),
    ("Duration (ms)", "Duración (ms)"),
    ("Themes", "Temas"),
    ("Fix these first:", "Corrige esto primero:"),
    ("Saved", "Guardado"),
    ("copy", "copia"),
    ("New theme", "Tema nuevo"),
    ("Delete the theme", "¿Eliminar el tema"),
    ("Delete", "Eliminar"),
    ("Cancel", "Cancelar"),
    ("Discard your changes to", "¿Descartar los cambios en"),
    ("Discard", "Descartar"),
    ("Keep Editing", "Seguir editando"),
    ("Choose", "Elegir"),
    ("None", "Ninguna"),
    ("Built in", "Incluida"),
    ("default", "predeterminado"),
    ("Left", "Izquierda"),
    ("Center", "Centro"),
    ("Right", "Derecha"),
    ("Top", "Arriba"),
    ("Middle", "Medio"),
    ("Bottom", "Abajo"),
    ("Fade", "Fundido"),
    ("Slide up", "Deslizar hacia arriba"),
    ("Cut", "Corte"),
    ("Logo (blank to logo)", "Logo (pantalla con logo)"),
    ("Background image", "Imagen de fondo"),
    ("Choose…", "Elegir…"),
    ("Text", "Texto"),
    ("Weight", "Grosor"),
    ("Sizes and widths are in % of the screen height; colors are #rrggbb or #rrggbbaa.", "Tamaños y grosores en % de la altura de la pantalla; colores como #rrggbb o #rrggbbaa."),
    ("Position", "Posición"),
    ("Across", "Horizontal"),
    ("Down", "Vertical"),
    ("Background", "Fondo"),
    ("Transition", "Transición"),
    ("Blanked to the logo:", "Con la pantalla en logo:"),
    ("An approximation: the projector shows the real thing.", "Una aproximación: el proyector muestra el resultado real."),
    ("Duplicate", "Duplicar"),
    ("Delete…", "Eliminar…"),
    ("The default theme", "El tema predeterminado"),
    ("Make default", "Hacer predeterminado"),
    ("Unsaved changes", "Cambios sin guardar"),
    ("Save  ⌘S", "Guardar  ⌘S"),
    ("Settings", "Ajustes"),
    ("Use a number from 1024 to 65535.", "Usa un número entre 1024 y 65535."),
    ("Language", "Idioma"),
    ("Used for everything that has no theme of its own. Edit themes with ⌘T.", "Se usa para todo lo que no tiene un tema propio. Edita los temas con ⌘T."),
    ("No second display", "No hay segunda pantalla"),
    ("Choose it in the live pane; Asaf remembers it and goes back to it when it's plugged in.", "Elígela en el panel en vivo; Asaf la recuerda y vuelve a ella cuando se conecta."),
    ("Display server port", "Puerto del servidor de pantalla"),
    ("Takes effect the next time Asaf starts. ASAF_PORT overrides it.", "Se aplica la próxima vez que se abra Asaf. ASAF_PORT tiene prioridad."),
    ("Migrate from OpenLP", "Migrar desde OpenLP"),
    ("Imports every song and Bible from an OpenLP data folder in one step. OpenLP's files aren't changed; songs and Bibles already here are skipped.", "Importa todas las canciones y Biblias de una carpeta de datos de OpenLP en un paso. Los archivos de OpenLP no se modifican; se omiten las canciones y Biblias que ya están aquí."),
    ("Import from OpenLP…", "Importar desde OpenLP…"),
    ("Import from OpenLP", "Importar desde OpenLP"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `.t("…")` in the console's source.
    fn used_strings() -> Vec<String> {
        let sources = [
            include_str!("console.rs"),
            include_str!("service.rs"),
            include_str!("bible.rs"),
            include_str!("theme_editor.rs"),
            include_str!("settings.rs"),
            include_str!("transfer.rs"),
            include_str!("slides.rs"),
        ];
        let mut out = Vec::new();
        for src in sources {
            for marker in [".t(\"", "en(\""] {
                let mut at = 0;
                while let Some(i) = src[at..].find(marker) {
                    let start = at + i;
                    at = start + marker.len();
                    // `en(` must be the function, not the end of another name.
                    let prev = src[..start].chars().next_back().unwrap_or(' ');
                    if marker.starts_with('e') && (prev.is_alphanumeric() || prev == '_') {
                        continue;
                    }
                    let end = at + src[at..].find("\")").expect("closing quote");
                    out.push(src[at..end].replace("\\\"", "\""));
                    at = end;
                }
            }
        }
        out
    }

    #[test]
    fn every_string_has_a_spanish_translation() {
        let used = used_strings();
        assert!(used.len() > 50, "found {} strings", used.len());
        let missing: Vec<_> = used.iter().filter(|s| !SPANISH_BY_ENGLISH.contains_key(s.as_str())).collect();
        assert!(missing.is_empty(), "no Spanish for: {missing:#?}");
    }

    #[test]
    fn the_table_has_no_duplicates_or_blanks() {
        let mut seen = std::collections::HashSet::new();
        for (en, es) in SPANISH {
            assert!(seen.insert(*en), "duplicate entry {en:?}");
            assert!(!es.trim().is_empty(), "blank translation for {en:?}");
        }
        let used = used_strings();
        let unused: Vec<_> = SPANISH.iter().filter(|(en, _)| !used.iter().any(|u| u == en)).collect();
        assert!(unused.is_empty(), "unused entries: {unused:#?}");
    }

    #[test]
    fn english_is_the_identity_and_spanish_translates() {
        assert_eq!(Lang(Language::English).t("Songs"), "Songs");
        assert_eq!(Lang(Language::Spanish).t("Songs"), "Canciones");
        assert_eq!(Lang(Language::Spanish).t("not in the table"), "not in the table");
    }
}
