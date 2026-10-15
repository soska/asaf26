//! Table-driven tests: real references as operators type them, in English
//! and Spanish, against their expected ranges in compact OSIS form.

use super::*;
use crate::bible::{BOOKS, BookNames};

fn check(names: &BookNames, cases: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for (input, expected) in cases {
        match parse_reference(input, names) {
            Ok(r) if osis_list(&r) == *expected => {}
            Ok(r) => failures.push(format!("{input:?}: got {:?}, expected {expected:?}", osis_list(&r))),
            Err(e) => failures.push(format!("{input:?}: error {e}, expected {expected:?}")),
        }
    }
    assert!(failures.is_empty(), "{} of {} failed:\n{}", failures.len(), cases.len(), failures.join("\n"));
}

const ENGLISH: &[(&str, &str)] = &[
    // The classics.
    ("John 3:16", "John.3.16"),
    ("Jn 3:16", "John.3.16"),
    ("jn 3:16", "John.3.16"),
    ("JN 3:16", "John.3.16"),
    ("Jn 3:16-18", "John.3.16-18"),
    ("Jn 3.16", "John.3.16"),
    ("Jn. 3:16", "John.3.16"),
    ("John 3 : 16 - 18", "John.3.16-18"),
    ("John 3:16–18", "John.3.16-18"),
    ("John 3:16—18", "John.3.16-18"),
    ("John 3:16 to 18", "John.3.16-18"),
    ("John 3:16, 18", "John.3.16; John.3.18"),
    ("John 3:16 and 18", "John.3.16; John.3.18"),
    ("John 3:16 and Romans 8:28", "John.3.16; Rom.8.28"),
    ("John 3:16 and 1 John 4:8", "John.3.16; 1John.4.8"),
    ("John 3:16,17,18", "John.3.16; John.3.17; John.3.18"),
    ("John 3:16a", "John.3.16"),
    ("John 3:16b-18", "John.3.16-18"),
    ("John 3:16-", "John.3.16-end"),
    ("John 3:16ff", "John.3.16-end"),
    ("John 3", "John.3"),
    ("John 3-4", "John.3-4"),
    ("John 14:1-6", "John.14.1-6"),
    ("John 1:1-14", "John.1.1-14"),
    ("John 11:35", "John.11.35"),
    ("John 10:10", "John.10.10"),
    ("John 15:13", "John.15.13"),
    ("Genesis 1:1", "Gen.1.1"),
    ("Gen 1:1-3", "Gen.1.1-3"),
    ("Gn 1:30-2:3", "Gen.1.30-2.3"),
    ("Gen 1-2", "Gen.1-2"),
    ("Gen 1-2:3", "Gen.1.1-2.3"),
    ("Gen 50:20", "Gen.50.20"),
    ("Exodus 20:1-17", "Exod.20.1-17"),
    ("Ex 3:14", "Exod.3.14"),
    ("Exod 14:14", "Exod.14.14"),
    ("Leviticus 19:18", "Lev.19.18"),
    ("Lev 19:18", "Lev.19.18"),
    ("Numbers 6:24-26", "Num.6.24-26"),
    ("Num 6:24-26", "Num.6.24-26"),
    ("Deuteronomy 6:4-9", "Deut.6.4-9"),
    ("Deut 31:6", "Deut.31.6"),
    ("Dt 6:5", "Deut.6.5"),
    ("Joshua 1:9", "Josh.1.9"),
    ("Josh 24:15", "Josh.24.15"),
    ("Judges 6:12", "Judg.6.12"),
    ("Ruth 1:16-17", "Ruth.1.16-17"),
    ("1 Samuel 16:7", "1Sam.16.7"),
    ("1 Sam 17:45", "1Sam.17.45"),
    ("1Sam 3:10", "1Sam.3.10"),
    ("I Samuel 16:7", "1Sam.16.7"),
    ("First Samuel 16:7", "1Sam.16.7"),
    ("2 Samuel 22:2-3", "2Sam.22.2-3"),
    ("1 Kings 18:21", "1Kgs.18.21"),
    ("1 Ki 19:12", "1Kgs.19.12"),
    ("2 Kings 6:16", "2Kgs.6.16"),
    ("II Kings 6:17", "2Kgs.6.17"),
    ("1 Chronicles 16:34", "1Chr.16.34"),
    ("1 Chr 4:10", "1Chr.4.10"),
    ("2 Chronicles 7:14", "2Chr.7.14"),
    ("2 Chr 7:14", "2Chr.7.14"),
    ("Ezra 7:10", "Ezra.7.10"),
    ("Nehemiah 8:10", "Neh.8.10"),
    ("Neh 8:10", "Neh.8.10"),
    ("Esther 4:14", "Esth.4.14"),
    ("Job 19:25", "Job.19.25"),
    ("Job 1:21", "Job.1.21"),
    ("Psalm 23", "Ps.23"),
    ("Psalms 23", "Ps.23"),
    ("Ps 23", "Ps.23"),
    ("Ps 23:1-6", "Ps.23.1-6"),
    ("Ps 23; 91:1-4", "Ps.23; Ps.91.1-4"),
    ("Ps 46:10", "Ps.46.10"),
    ("Ps 51:10-12", "Ps.51.10-12"),
    ("Ps 91", "Ps.91"),
    ("Ps 100", "Ps.100"),
    ("Psalm 103:1-5", "Ps.103.1-5"),
    ("Psalm 119:105", "Ps.119.105"),
    ("Ps 119:9-16", "Ps.119.9-16"),
    ("Psalm 121", "Ps.121"),
    ("Ps 139:13-14", "Ps.139.13-14"),
    ("Psa 150", "Ps.150"),
    ("Ps 1; 2; 3", "Ps.1; Ps.2; Ps.3"),
    ("Ps 1, 2", "Ps.1; Ps.2"),
    ("Proverbs 3:5-6", "Prov.3.5-6"),
    ("Prov 3:5-6", "Prov.3.5-6"),
    ("Pr 22:6", "Prov.22.6"),
    ("Prov 31:10-31", "Prov.31.10-31"),
    ("Ecclesiastes 3:1-8", "Eccl.3.1-8"),
    ("Eccl 3:11", "Eccl.3.11"),
    ("Song of Solomon 2:4", "Song.2.4"),
    ("Song of Songs 8:6-7", "Song.8.6-7"),
    ("Song 2:4", "Song.2.4"),
    ("Isaiah 40:31", "Isa.40.31"),
    ("Isa 9:6", "Isa.9.6"),
    ("Is 53:5", "Isa.53.5"),
    ("Isa 53", "Isa.53"),
    ("Isaiah 41:10", "Isa.41.10"),
    ("Isa 6:8", "Isa.6.8"),
    ("Jeremiah 29:11", "Jer.29.11"),
    ("Jer 29:11-13", "Jer.29.11-13"),
    ("Lamentations 3:22-23", "Lam.3.22-23"),
    ("Lam 3:22-23", "Lam.3.22-23"),
    ("Ezekiel 36:26", "Ezek.36.26"),
    ("Ezek 37:1-14", "Ezek.37.1-14"),
    ("Daniel 3:17-18", "Dan.3.17-18"),
    ("Dan 6:22", "Dan.6.22"),
    ("Hosea 6:6", "Hos.6.6"),
    ("Joel 2:28", "Joel.2.28"),
    ("Amos 5:24", "Amos.5.24"),
    ("Obadiah 15", "Obad.1.15"),
    ("Obad 1:15", "Obad.1.15"),
    ("Jonah 2:9", "Jonah.2.9"),
    ("Micah 6:8", "Mic.6.8"),
    ("Mic 5:2", "Mic.5.2"),
    ("Nahum 1:7", "Nah.1.7"),
    ("Habakkuk 3:17-19", "Hab.3.17-19"),
    ("Hab 2:4", "Hab.2.4"),
    ("Zephaniah 3:17", "Zeph.3.17"),
    ("Zeph 3:17", "Zeph.3.17"),
    ("Haggai 2:9", "Hag.2.9"),
    ("Zechariah 4:6", "Zech.4.6"),
    ("Zech 9:9", "Zech.9.9"),
    ("Malachi 3:10", "Mal.3.10"),
    ("Mal 4:2", "Mal.4.2"),
    ("Matthew 5:3-12", "Matt.5.3-12"),
    ("Matt 6:9-13", "Matt.6.9-13"),
    ("Mt 6:33", "Matt.6.33"),
    ("Mt 11:28-30", "Matt.11.28-30"),
    ("Matt 28:18-20", "Matt.28.18-20"),
    ("Mt 5-7", "Matt.5-7"),
    ("Matthew 22:37-39", "Matt.22.37-39"),
    ("Mark 10:45", "Mark.10.45"),
    ("Mk 16:15", "Mark.16.15"),
    ("Mark 1:1-8", "Mark.1.1-8"),
    ("Luke 2:1-20", "Luke.2.1-20"),
    ("Lk 2:10-11", "Luke.2.10-11"),
    ("Luke 15:11-32", "Luke.15.11-32"),
    ("Luke 1:37", "Luke.1.37"),
    ("Acts 1:8", "Acts.1.8"),
    ("Acts 2:38", "Acts.2.38"),
    ("Acts 2:42-47", "Acts.2.42-47"),
    ("Acts 4:12", "Acts.4.12"),
    ("Romans 3:23", "Rom.3.23"),
    ("Rom 5:8", "Rom.5.8"),
    ("Rom 6:23", "Rom.6.23"),
    ("Rom 8:28", "Rom.8.28"),
    ("Rom 8:38-39", "Rom.8.38-39"),
    ("Ro 10:9-10", "Rom.10.9-10"),
    ("Rom 12:1-2", "Rom.12.1-2"),
    ("Romans 3:23; 6:23", "Rom.3.23; Rom.6.23"),
    ("1 Corinthians 13", "1Cor.13"),
    ("1 Cor 13:4-7", "1Cor.13.4-7"),
    ("1Cor 10:13", "1Cor.10.13"),
    ("1 Co 15:3-4", "1Cor.15.3-4"),
    ("First Corinthians 13:13", "1Cor.13.13"),
    ("2 Corinthians 5:17", "2Cor.5.17"),
    ("2 Cor 12:9", "2Cor.12.9"),
    ("II Cor 5:17", "2Cor.5.17"),
    ("Galatians 2:20", "Gal.2.20"),
    ("Gal 5:22-23", "Gal.5.22-23"),
    ("Ephesians 2:8-9", "Eph.2.8-9"),
    ("Eph 6:10-18", "Eph.6.10-18"),
    ("Eph 4:32", "Eph.4.32"),
    ("Philippians 4:13", "Phil.4.13"),
    ("Phil 4:6-7", "Phil.4.6-7"),
    ("Php 1:6", "Phil.1.6"),
    ("Colossians 3:23", "Col.3.23"),
    ("Col 3:12-17", "Col.3.12-17"),
    ("1 Thessalonians 5:16-18", "1Thess.5.16-18"),
    ("1 Thess 4:16-17", "1Thess.4.16-17"),
    ("2 Thessalonians 3:3", "2Thess.3.3"),
    ("1 Timothy 4:12", "1Tim.4.12"),
    ("1 Tim 6:6", "1Tim.6.6"),
    ("2 Timothy 3:16-17", "2Tim.3.16-17"),
    ("2 Tim 1:7", "2Tim.1.7"),
    ("Titus 2:11-14", "Titus.2.11-14"),
    ("Philemon 6", "Phlm.1.6"),
    ("Phlm 4-7", "Phlm.1.4-7"),
    ("Hebrews 11:1", "Heb.11.1"),
    ("Heb 11:6", "Heb.11.6"),
    ("Heb 12:1-2", "Heb.12.1-2"),
    ("Heb 4:12", "Heb.4.12"),
    ("Heb 13:8", "Heb.13.8"),
    ("James 1:2-4", "Jas.1.2-4"),
    ("Jas 1:5", "Jas.1.5"),
    ("James 4:7-8", "Jas.4.7-8"),
    ("1 Peter 5:7", "1Pet.5.7"),
    ("1 Pet 2:9", "1Pet.2.9"),
    ("1 Pe 3:15", "1Pet.3.15"),
    ("2 Peter 3:9", "2Pet.3.9"),
    ("1 John 1:9", "1John.1.9"),
    ("1 Jn 4:7-8", "1John.4.7-8"),
    ("1Jn 4:19", "1John.4.19"),
    ("I John 4:8", "1John.4.8"),
    ("2 John 6", "2John.1.6"),
    ("3 John 2", "3John.1.2"),
    ("3 Jn 4", "3John.1.4"),
    ("Jude 24-25", "Jude.1.24-25"),
    ("Jude 1:24-25", "Jude.1.24-25"),
    ("Revelation 3:20", "Rev.3.20"),
    ("Rev 21:1-4", "Rev.21.1-4"),
    ("Rev 22:20", "Rev.22.20"),
    ("Rev 21-22", "Rev.21-22"),
    // Lists across books.
    ("John 3:16; Rom 5:8", "John.3.16; Rom.5.8"),
    ("Jn 3:16, Rom 5:8", "John.3.16; Rom.5.8"),
    ("Rom 3:23; 6:23; 10:9", "Rom.3.23; Rom.6.23; Rom.10.9"),
    ("Jn 14:6; Acts 4:12; 1 Tim 2:5", "John.14.6; Acts.4.12; 1Tim.2.5"),
    ("Ps 23; 1 John 4", "Ps.23; 1John.4"),
    // Prefixes of full names.
    ("Gene 1:1", "Gen.1.1"),
    ("Revel 1:8", "Rev.1.8"),
    ("Philip 4:13", "Phil.4.13"),
    ("Deuter 6:4", "Deut.6.4"),
    ("Ecclesi 12:1", "Eccl.12.1"),
];

const SPANISH: &[(&str, &str)] = &[
    ("Juan 3:16", "John.3.16"),
    ("Jn 3:16-18", "John.3.16-18"),
    ("juan 3.16", "John.3.16"),
    ("JUAN 3:16", "John.3.16"),
    ("San Juan 3:16", "John.3.16"),
    ("Juan 3:16 al 18", "John.3.16-18"),
    ("Juan 3:16 a 18", "John.3.16-18"),
    ("Juan 3:16 hasta 18", "John.3.16-18"),
    ("Juan 3:16 y 18", "John.3.16; John.3.18"),
    ("Juan 3:16 y Romanos 8:28", "John.3.16; Rom.8.28"),
    ("Sal 23 y Sal 91", "Ps.23; Ps.91"),
    ("Juan 3:16ss", "John.3.16-end"),
    ("Juan 3 v 16", "John.3.16"),
    ("Juan 3", "John.3"),
    ("Juan 14:6", "John.14.6"),
    ("Juan 1:1-5", "John.1.1-5"),
    ("Juan 1:12", "John.1.12"),
    ("Juan 10:10", "John.10.10"),
    ("Juan 11:25-26", "John.11.25-26"),
    ("Juan 13:34-35", "John.13.34-35"),
    ("Juan 15:5", "John.15.5"),
    ("Juan 20:31", "John.20.31"),
    ("Génesis 1:1", "Gen.1.1"),
    ("Genesis 1:1", "Gen.1.1"),
    ("Gn 1:30-2:3", "Gen.1.30-2.3"),
    ("Gén 1:26-27", "Gen.1.26-27"),
    ("Gn 12:1-3", "Gen.12.1-3"),
    ("Gn 50:20", "Gen.50.20"),
    ("Éxodo 20:1-17", "Exod.20.1-17"),
    ("Éx 14:14", "Exod.14.14"),
    ("Ex 3:14", "Exod.3.14"),
    ("Exodo 15:2", "Exod.15.2"),
    ("Levítico 19:18", "Lev.19.18"),
    ("Lv 19:2", "Lev.19.2"),
    ("Números 6:24-26", "Num.6.24-26"),
    ("Nm 6:24-26", "Num.6.24-26"),
    ("Num 23:19", "Num.23.19"),
    ("Deuteronomio 6:4-9", "Deut.6.4-9"),
    ("Dt 31:8", "Deut.31.8"),
    ("Josué 1:9", "Josh.1.9"),
    ("Jos 1:8", "Josh.1.8"),
    ("Josue 24:15", "Josh.24.15"),
    ("Jueces 6:12", "Judg.6.12"),
    ("Jue 7:20", "Judg.7.20"),
    ("Rut 1:16", "Ruth.1.16"),
    ("1 Samuel 16:7", "1Sam.16.7"),
    ("1 S 17:47", "1Sam.17.47"),
    ("1S 3:10", "1Sam.3.10"),
    ("1 Sam 15:22", "1Sam.15.22"),
    ("2 Samuel 22:2", "2Sam.22.2"),
    ("2 S 7:22", "2Sam.7.22"),
    ("1 Reyes 18:21", "1Kgs.18.21"),
    ("1 R 19:11-12", "1Kgs.19.11-12"),
    ("1R 8:27", "1Kgs.8.27"),
    ("2 Reyes 6:16-17", "2Kgs.6.16-17"),
    ("2 R 2:11", "2Kgs.2.11"),
    ("Segundo de Reyes 6:16", "2Kgs.6.16"),
    ("1 Crónicas 16:34", "1Chr.16.34"),
    ("1 Cr 4:10", "1Chr.4.10"),
    ("2 Crónicas 7:14", "2Chr.7.14"),
    ("2 Cr 7:14", "2Chr.7.14"),
    ("2 Cronicas 20:15", "2Chr.20.15"),
    ("Esdras 7:10", "Ezra.7.10"),
    ("Esd 3:11", "Ezra.3.11"),
    ("Nehemías 8:10", "Neh.8.10"),
    ("Neh 8:10", "Neh.8.10"),
    ("Ester 4:14", "Esth.4.14"),
    ("Est 4:16", "Esth.4.16"),
    ("Job 19:25", "Job.19.25"),
    ("Job 42:2", "Job.42.2"),
    ("Salmos 23", "Ps.23"),
    ("Salmo 23", "Ps.23"),
    ("Sal 23", "Ps.23"),
    ("Sal 23; 91:1-4", "Ps.23; Ps.91.1-4"),
    ("Sal 23:1", "Ps.23.1"),
    ("Sal 27:1", "Ps.27.1"),
    ("Sal 37:4-5", "Ps.37.4-5"),
    ("Sal 46:1", "Ps.46.1"),
    ("Sal 51", "Ps.51"),
    ("Sal 84:10", "Ps.84.10"),
    ("Sal 91", "Ps.91"),
    ("Sal 100:4", "Ps.100.4"),
    ("Sal 103:1-5", "Ps.103.1-5"),
    ("Sal 119:105", "Ps.119.105"),
    ("Sal 121:1-2", "Ps.121.1-2"),
    ("Sal 139:23-24", "Ps.139.23-24"),
    ("Sal 150:6", "Ps.150.6"),
    ("Sal 1 y 2", "Ps.1; Ps.2"),
    ("Sal 120-134", "Ps.120-134"),
    ("Proverbios 3:5-6", "Prov.3.5-6"),
    ("Pr 3:5-6", "Prov.3.5-6"),
    ("Prov 4:23", "Prov.4.23"),
    ("Pr 22:6", "Prov.22.6"),
    ("Eclesiastés 3:1", "Eccl.3.1"),
    ("Ec 12:13", "Eccl.12.13"),
    ("Ecl 3:11", "Eccl.3.11"),
    ("Cantares 2:4", "Song.2.4"),
    ("Cnt 8:7", "Song.8.7"),
    ("Cantar de los Cantares 8:6", "Song.8.6"),
    ("Isaías 40:31", "Isa.40.31"),
    ("Is 9:6", "Isa.9.6"),
    ("Isaias 53:5", "Isa.53.5"),
    ("Is 41:10", "Isa.41.10"),
    ("Is 43:18-19", "Isa.43.18-19"),
    ("Is 55:8-9", "Isa.55.8-9"),
    ("Is 53", "Isa.53"),
    ("Jeremías 29:11", "Jer.29.11"),
    ("Jer 33:3", "Jer.33.3"),
    ("Jr 17:7-8", "Jer.17.7-8"),
    ("Lamentaciones 3:22-23", "Lam.3.22-23"),
    ("Lm 3:22-23", "Lam.3.22-23"),
    ("Ezequiel 36:26", "Ezek.36.26"),
    ("Ez 37:1-14", "Ezek.37.1-14"),
    ("Daniel 3:17-18", "Dan.3.17-18"),
    ("Dn 6:22", "Dan.6.22"),
    ("Oseas 6:3", "Hos.6.3"),
    ("Os 4:6", "Hos.4.6"),
    ("Joel 2:28-29", "Joel.2.28-29"),
    ("Jl 2:25", "Joel.2.25"),
    ("Amós 5:24", "Amos.5.24"),
    ("Abdías 15", "Obad.1.15"),
    ("Abd 1:4", "Obad.1.4"),
    ("Jonás 2:9", "Jonah.2.9"),
    ("Miqueas 6:8", "Mic.6.8"),
    ("Mi 7:18", "Mic.7.18"),
    ("Nahúm 1:7", "Nah.1.7"),
    ("Habacuc 3:17-19", "Hab.3.17-19"),
    ("Hab 2:14", "Hab.2.14"),
    ("Sofonías 3:17", "Zeph.3.17"),
    ("Sof 3:17", "Zeph.3.17"),
    ("Hageo 2:9", "Hag.2.9"),
    ("Hag 1:5", "Hag.1.5"),
    ("Zacarías 4:6", "Zech.4.6"),
    ("Zac 4:6", "Zech.4.6"),
    ("Malaquías 3:10", "Mal.3.10"),
    ("Mal 3:6", "Mal.3.6"),
    ("Mateo 5:3-12", "Matt.5.3-12"),
    ("Mt 6:9-13", "Matt.6.9-13"),
    ("Mt 6:33", "Matt.6.33"),
    ("Mateo 11:28", "Matt.11.28"),
    ("Mt 28:19-20", "Matt.28.19-20"),
    ("San Mateo 5:14", "Matt.5.14"),
    ("Marcos 16:15", "Mark.16.15"),
    ("Mr 10:45", "Mark.10.45"),
    ("Mc 11:24", "Mark.11.24"),
    ("Lucas 2:10-11", "Luke.2.10-11"),
    ("Lc 1:37", "Luke.1.37"),
    ("Lc 15:11-32", "Luke.15.11-32"),
    ("Lucas 19:10", "Luke.19.10"),
    ("Hechos 1:8", "Acts.1.8"),
    ("Hch 2:38", "Acts.2.38"),
    ("Hch 4:12", "Acts.4.12"),
    ("Hechos 16:31", "Acts.16.31"),
    ("Romanos 3:23", "Rom.3.23"),
    ("Ro 5:8", "Rom.5.8"),
    ("Ro 6:23", "Rom.6.23"),
    ("Rom 8:28", "Rom.8.28"),
    ("Ro 8:31-39", "Rom.8.31-39"),
    ("Ro 10:9-10", "Rom.10.9-10"),
    ("Ro 12:1-2", "Rom.12.1-2"),
    ("Romanos 3:23; 6:23; 10:9", "Rom.3.23; Rom.6.23; Rom.10.9"),
    ("1 Corintios 13", "1Cor.13"),
    ("1 Co 13:4-7", "1Cor.13.4-7"),
    ("1Co 10:13", "1Cor.10.13"),
    ("1 Cor 15:58", "1Cor.15.58"),
    ("Primera de Corintios 13:13", "1Cor.13.13"),
    ("1ra de Corintios 13:4", "1Cor.13.4"),
    ("1ra Corintios 13:4", "1Cor.13.4"),
    ("2 Corintios 5:17", "2Cor.5.17"),
    ("2 Co 12:9", "2Cor.12.9"),
    ("2da Corintios 5:17", "2Cor.5.17"),
    ("Segunda de Corintios 4:18", "2Cor.4.18"),
    ("Gálatas 2:20", "Gal.2.20"),
    ("Gá 5:22-23", "Gal.5.22-23"),
    ("Gal 6:9", "Gal.6.9"),
    ("Efesios 2:8-9", "Eph.2.8-9"),
    ("Ef 6:10-18", "Eph.6.10-18"),
    ("Ef 3:20", "Eph.3.20"),
    ("Filipenses 4:13", "Phil.4.13"),
    ("Fil 4:6-7", "Phil.4.6-7"),
    ("Flp 1:6", "Phil.1.6"),
    ("Fil 4:19", "Phil.4.19"),
    ("Colosenses 3:23", "Col.3.23"),
    ("Col 3:16", "Col.3.16"),
    ("1 Tesalonicenses 5:16-18", "1Thess.5.16-18"),
    ("1 Ts 4:16-17", "1Thess.4.16-17"),
    ("2 Tesalonicenses 3:3", "2Thess.3.3"),
    ("1 Timoteo 4:12", "1Tim.4.12"),
    ("1 Ti 6:6", "1Tim.6.6"),
    ("2 Timoteo 3:16-17", "2Tim.3.16-17"),
    ("2 Ti 1:7", "2Tim.1.7"),
    ("Tito 2:11", "Titus.2.11"),
    ("Tit 3:5", "Titus.3.5"),
    ("Filemón 6", "Phlm.1.6"),
    ("Flm 4-7", "Phlm.1.4-7"),
    ("Hebreos 11:1", "Heb.11.1"),
    ("He 11:6", "Heb.11.6"),
    ("Heb 12:1-2", "Heb.12.1-2"),
    ("He 4:12", "Heb.4.12"),
    ("He 13:8", "Heb.13.8"),
    ("Santiago 1:2-4", "Jas.1.2-4"),
    ("Stg 1:5", "Jas.1.5"),
    ("Sant 4:7-8", "Jas.4.7-8"),
    ("1 Pedro 5:7", "1Pet.5.7"),
    ("1 P 2:9", "1Pet.2.9"),
    ("1Pe 3:15", "1Pet.3.15"),
    ("2 Pedro 3:9", "2Pet.3.9"),
    ("1 Juan 1:9", "1John.1.9"),
    ("1 Jn 4:7-8", "1John.4.7-8"),
    ("1Jn 4:19", "1John.4.19"),
    ("Primera de Juan 4:8", "1John.4.8"),
    ("Primera Juan 4:8", "1John.4.8"),
    ("1ra. de Juan 4:8", "1John.4.8"),
    ("I Juan 4:8", "1John.4.8"),
    ("1 Juan 4", "1John.4"),
    ("2 Juan 6", "2John.1.6"),
    ("3 Juan 2", "3John.1.2"),
    ("Tercera de Juan 4", "3John.1.4"),
    ("Judas 24-25", "Jude.1.24-25"),
    ("Jud 3", "Jude.1.3"),
    ("Apocalipsis 3:20", "Rev.3.20"),
    ("Ap 21:1-4", "Rev.21.1-4"),
    ("Apoc 22:20", "Rev.22.20"),
    ("Ap 21-22", "Rev.21-22"),
    // Lists across books.
    ("Juan 3:16; Ro 5:8", "John.3.16; Rom.5.8"),
    ("Jn 14:6; Hch 4:12; 1 Ti 2:5", "John.14.6; Acts.4.12; 1Tim.2.5"),
    ("Sal 23; 1 Juan 4", "Ps.23; 1John.4"),
    ("Juan 3:16 y 1 Juan 4:8", "John.3.16; 1John.4.8"),
    // Prefixes of full names.
    ("Apocal 1:8", "Rev.1.8"),
    ("Filip 4:13", "Phil.4.13"),
    ("Deuteron 6:4", "Deut.6.4"),
    ("Lament 3:23", "Lam.3.23"),
];

#[test]
fn english_references() {
    check(&BookNames::new("en", &[]), ENGLISH);
}

#[test]
fn spanish_references() {
    check(&BookNames::new("es", &[]), SPANISH);
}

#[test]
fn either_language_works_in_any_translation() {
    // An operator with a Spanish Bible loaded may still type "John 3:16".
    check(&BookNames::new("es", &[]), ENGLISH);
    check(&BookNames::new("en", &[]), SPANISH);
}

#[test]
fn table_is_large() {
    assert!(ENGLISH.len() + SPANISH.len() >= 400, "{}", ENGLISH.len() + SPANISH.len());
}

/// Every name and abbreviation in the book table, in both languages, with
/// and without a space and a trailing dot, resolves to its own book.
#[test]
fn every_book_name_and_abbreviation() {
    for lang in ["en", "es"] {
        let names = BookNames::new(lang, &[]);
        let mut cases: Vec<(String, String)> = Vec::new();
        for b in &BOOKS {
            for name in b.en.iter().chain(b.es) {
                let (spec, expected) = if b.chapters == 1 {
                    ("5", format!("{}.1.5", b.osis))
                } else {
                    ("2:5-7", format!("{}.2.5-7", b.osis))
                };
                cases.push((format!("{name} {spec}"), expected.clone()));
                cases.push((format!("{name}. {spec}"), expected.clone()));
                cases.push((format!("{}{spec}", name.to_uppercase()), expected.clone()));
                cases.push((format!("{} {spec}", name.to_lowercase()), expected));
            }
        }
        let cases: Vec<(&str, &str)> = cases.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        assert!(cases.len() > 1000, "{lang}: {}", cases.len());
        check(&names, &cases);
    }
}

#[test]
fn english_and_spanish_abbreviations_never_conflict() {
    let mut conflicts = Vec::new();
    for (i, a) in BOOKS.iter().enumerate() {
        for name in a.en {
            for (j, b) in BOOKS.iter().enumerate() {
                if i != j && b.es.iter().any(|n| crate::bible::normalize_book_name(n) == crate::bible::normalize_book_name(name)) {
                    conflicts.push(format!("{name}: en {} / es {}", a.osis, b.osis));
                }
            }
        }
    }
    // None today; if one appears, the translation's language decides.
    assert!(conflicts.is_empty(), "{conflicts:?}");
}

#[test]
fn errors_explain_what_is_wrong() {
    let names = BookNames::new("es", &[]);
    for (input, expected) in [
        ("", RefError::Empty),
        ("   ", RefError::Empty),
        ("3:16", RefError::NoBook),
        ("Xyz 3:16", RefError::UnknownBook("Xyz".into())),
        ("Juan", RefError::NoChapter("Juan".into())),
        ("Juan 22", RefError::ChapterOutOfRange { book: "Juan".into(), chapter: 22, chapters: 21 }),
        ("Sal 151", RefError::ChapterOutOfRange { book: "Salmos".into(), chapter: 151, chapters: 150 }),
        ("Gn 2:3-1:30", RefError::Backwards("Génesis 2:3-1:30".into())),
        ("Juan 3:18-16", RefError::Backwards("Juan 3:18-16".into())),
        ("Juan 0", RefError::VerseZero),
        ("Juan 3:0", RefError::VerseZero),
        ("Juan 3:", RefError::Incomplete),
        ("Juan 3:16 hola", RefError::Unexpected("hola".into())),
        ("Juan 3:16 #", RefError::Unexpected("#".into())),
    ] {
        assert_eq!(parse_reference(input, &names), Err(expected), "{input:?}");
    }
    assert!(matches!(parse_reference("Ju 3:16", &names), Err(RefError::AmbiguousBook(_, _))));
    let msg = parse_reference("Ju 3:16", &names).unwrap_err().to_string();
    assert!(msg.contains("could be"), "{msg}");
    assert_eq!(
        parse_reference("Juan 22", &names).unwrap_err().to_string(),
        "Juan has 21 chapters, not 22"
    );
}

#[test]
fn trailing_separator_while_typing_is_fine() {
    let names = BookNames::new("en", &[]);
    assert_eq!(osis_list(&parse_reference("Jn 3:16,", &names).unwrap()), "John.3.16");
    assert_eq!(osis_list(&parse_reference("Ps 23;", &names).unwrap()), "Ps.23");
}

#[test]
fn formats_references_for_display() {
    let es = BookNames::new("es", &[]);
    let en = BookNames::new("en", &[]);
    let none = |_, _| None;
    for (names, input, expected) in [
        (&es, "jn 3:16-18", "Juan 3:16-18"),
        (&es, "gn 1:30-2:3", "Génesis 1:30-2:3"),
        (&es, "sal 23; 91:1-4", "Salmos 23; 91:1-4"),
        (&es, "jn 3:16, 18", "Juan 3:16, 18"),
        (&es, "jn 3:16; ro 5:8", "Juan 3:16; Romanos 5:8"),
        (&es, "1 jn 4", "1 Juan 4"),
        (&es, "ap 21-22", "Apocalipsis 21-22"),
        (&es, "jud 3", "Judas 3"),
        (&en, "jn 3:16", "John 3:16"),
        (&en, "Ps 23; 24:1", "Psalms 23; 24:1"),
        (&en, "Gen 1-2:3", "Genesis 1:1-2:3"),
    ] {
        let ranges = parse_reference(input, names).unwrap();
        assert_eq!(format_reference(&ranges, names, none), expected, "{input}");
    }
    let ranges = parse_reference("Jn 3:16-", &en).unwrap();
    assert_eq!(format_reference(&ranges, &en, |_, _| Some(36)), "John 3:16-36");
}
