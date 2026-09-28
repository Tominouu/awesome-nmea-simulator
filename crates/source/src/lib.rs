//! Sources de données de nmeasim-rs (`docs/10`) : traces GPX et KML,
//! lecteur de trace, journal `.nmeasim` (D7) et rejeu.

pub mod gpx;
pub mod journal;
pub mod kml;
pub mod replay;
pub mod track;

pub use track::{PlayMode, SourceError, Track, TrackFile, TrackPlayer, TrackPoint};

/// Lit un fichier de trace selon son extension (`.gpx`, `.kml`).
pub fn read_track_file(
    path: &std::path::Path,
    kml_mode: kml::KmlMode,
) -> Result<TrackFile, SourceError> {
    let data = std::fs::read_to_string(path)
        .map_err(|e| SourceError(format!("{} : {e}", path.display())))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "kml" {
        kml::parse(&data, kml_mode)
    } else {
        gpx::parse(&data)
    }
}
