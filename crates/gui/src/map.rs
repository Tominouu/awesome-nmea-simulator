//! Carte 2D (Web Mercator) dessinée avec egui : carroyage, échelle, sillage,
//! route, destination, trace, navire, cap et vecteur de route fond.
//!
//! Gestes (`docs/09` §4) : glisser = déplacer, molette = zoom, clic droit =
//! destination (comportement legacy, position lue au moment du clic),
//! Maj + clic droit = ajouter un point de route, Ctrl + clic gauche =
//! repositionner le navire.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use nmeasim_core::geo::LatLon;
use nmeasim_core::units::mps_to_knots;
use nmeasim_sim::state::SimState;

const TILE: f64 = 256.0;

/// Action demandée par un geste.
#[derive(Debug, Clone, PartialEq)]
pub enum MapAction {
    /// Destination (clic droit).
    Destination(LatLon),
    /// Point de route ajouté (Maj + clic droit).
    AddWaypoint(LatLon),
    /// Repositionner le navire (Ctrl + clic gauche).
    MoveVessel(LatLon),
}

/// État de la vue.
pub struct MapView {
    /// Centre.
    pub center: LatLon,
    /// Zoom (niveau de tuile, fractionnaire).
    pub zoom: f64,
    /// Suivi du navire.
    pub follow: bool,
    /// Position du pointeur.
    pub hover: Option<LatLon>,
}

impl MapView {
    /// Nouvelle vue.
    pub fn new(center: LatLon, zoom: f64, follow: bool) -> Self {
        Self {
            center,
            zoom,
            follow,
            hover: None,
        }
    }

    fn scale(&self) -> f64 {
        TILE * 2f64.powf(self.zoom)
    }

    /// Projection monde → pixels (origine arbitraire).
    fn world(&self, p: LatLon) -> (f64, f64) {
        let s = self.scale();
        let x = (p.lon + 180.0) / 360.0 * s;
        let phi = p.lat.clamp(-85.0, 85.0).to_radians();
        let y = (1.0 - (phi.tan() + 1.0 / phi.cos()).ln() / std::f64::consts::PI) / 2.0 * s;
        (x, y)
    }

    fn unworld(&self, x: f64, y: f64) -> LatLon {
        let s = self.scale();
        let lon = x / s * 360.0 - 180.0;
        let n = std::f64::consts::PI * (1.0 - 2.0 * y / s);
        LatLon::new(n.sinh().atan().to_degrees(), lon)
    }

    fn to_screen(&self, rect: Rect, p: LatLon) -> Pos2 {
        let (cx, cy) = self.world(self.center);
        let (x, y) = self.world(p);
        rect.center() + Vec2::new((x - cx) as f32, (y - cy) as f32)
    }

    fn screen_to_geo(&self, rect: Rect, pos: Pos2) -> LatLon {
        let (cx, cy) = self.world(self.center);
        let d = pos - rect.center();
        self.unworld(cx + f64::from(d.x), cy + f64::from(d.y))
    }

    /// Mètres par pixel au centre.
    fn meters_per_pixel(&self) -> f64 {
        40_075_016.686 * self.center.lat.to_radians().cos() / self.scale()
    }

    /// Dessine la carte et rend l'action éventuelle.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        s: &SimState,
        trail: &[LatLon],
        track: Option<&[LatLon]>,
        dark: bool,
        tiles: Option<&mut crate::tiles::TileCache>,
    ) -> Option<MapAction> {
        let size = ui.available_size().max(Vec2::new(200.0, 200.0));
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        if self.follow {
            self.center = s.vessel.position;
        }
        if resp.dragged() {
            let d = resp.drag_delta();
            let (cx, cy) = self.world(self.center);
            self.center = self.unworld(cx - f64::from(d.x), cy - f64::from(d.y));
            self.follow = false;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom + f64::from(scroll) / 200.0).clamp(2.0, 19.0);
            }
        }
        self.hover = resp.hover_pos().map(|p| self.screen_to_geo(rect, p));
        let painter = ui.painter_at(rect);
        let (bg, grid, text) = if dark {
            (
                Color32::from_rgb(12, 34, 56),
                Color32::from_rgb(40, 70, 100),
                Color32::from_gray(190),
            )
        } else {
            (
                Color32::from_rgb(200, 225, 240),
                Color32::from_rgb(150, 180, 205),
                Color32::from_gray(40),
            )
        };
        painter.rect_filled(rect, 0.0, bg);
        let with_tiles = tiles.is_some();
        if let Some(t) = tiles {
            t.draw(ui.ctx(), &painter, rect, self.world(self.center), self.zoom);
        }
        let grid = if with_tiles {
            Color32::from_rgba_unmultiplied(40, 70, 100, 70)
        } else {
            grid
        };
        let text = if with_tiles {
            Color32::from_gray(25)
        } else {
            text
        };

        // Carroyage : pas choisi pour ~100 px.
        let deg_per_px = 360.0 / self.scale();
        let target = deg_per_px * 100.0;
        let steps = [
            1e-4, 2e-4, 5e-4, 1e-3, 2e-3, 5e-3, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0,
            10.0, 20.0, 45.0,
        ];
        let step = steps.iter().copied().find(|s| *s >= target).unwrap_or(45.0);
        let tl = self.screen_to_geo(rect, rect.left_top());
        let br = self.screen_to_geo(rect, rect.right_bottom());
        let mut lon = (tl.lon / step).floor() * step;
        while lon <= br.lon {
            let x = self.to_screen(rect, LatLon::new(self.center.lat, lon)).x;
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0, grid),
            );
            painter.text(
                Pos2::new(x + 3.0, rect.bottom() - 14.0),
                Align2::LEFT_TOP,
                fmt_deg(lon, false),
                FontId::monospace(10.0),
                text,
            );
            lon += step;
        }
        let mut lat = (br.lat / step).floor() * step;
        while lat <= tl.lat {
            let y = self.to_screen(rect, LatLon::new(lat, self.center.lon)).y;
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0, grid),
            );
            painter.text(
                Pos2::new(rect.left() + 3.0, y + 2.0),
                Align2::LEFT_TOP,
                fmt_deg(lat, true),
                FontId::monospace(10.0),
                text,
            );
            lat += step;
        }

        // Trace chargée.
        if let Some(t) = track {
            let pts: Vec<Pos2> = t.iter().map(|p| self.to_screen(rect, *p)).collect();
            painter.add(Shape::line(
                pts.clone(),
                Stroke::new(2.0, Color32::from_rgb(230, 170, 40)),
            ));
            for p in pts {
                painter.circle_filled(p, 2.5, Color32::from_rgb(230, 170, 40));
            }
        }
        // Sillage.
        if trail.len() > 1 {
            let pts: Vec<Pos2> = trail.iter().map(|p| self.to_screen(rect, *p)).collect();
            painter.add(Shape::line(
                pts,
                Stroke::new(1.5, Color32::from_rgb(90, 200, 255)),
            ));
        }
        // Route.
        let route = &s.route;
        if !route.waypoints.is_empty() {
            let mut pts = vec![];
            if let Some(o) = &route.origin {
                pts.push(self.to_screen(rect, o.position));
            }
            pts.extend(
                route
                    .waypoints
                    .iter()
                    .map(|w| self.to_screen(rect, w.position)),
            );
            painter.add(Shape::dashed_line(
                &pts,
                Stroke::new(1.5, Color32::from_rgb(255, 120, 200)),
                6.0,
                4.0,
            ));
            for (i, w) in route.waypoints.iter().enumerate() {
                let p = self.to_screen(rect, w.position);
                let active = i == route.active;
                let c = if active {
                    Color32::from_rgb(255, 80, 80)
                } else {
                    Color32::from_rgb(255, 150, 200)
                };
                painter.circle_stroke(p, if active { 8.0 } else { 5.0 }, Stroke::new(2.0, c));
                painter.text(
                    p + Vec2::new(9.0, -9.0),
                    Align2::LEFT_BOTTOM,
                    &w.name,
                    FontId::proportional(12.0),
                    c,
                );
            }
        }
        if let Some(m) = &s.marked {
            let p = self.to_screen(rect, m.position);
            painter.rect_stroke(
                Rect::from_center_size(p, Vec2::splat(9.0)),
                0.0,
                Stroke::new(2.0, Color32::YELLOW),
                egui::StrokeKind::Middle,
            );
        }

        // Navire.
        let v = &s.vessel;
        let c = self.to_screen(rect, v.position);
        let h = v.heading as f32;
        let rot =
            |x: f32, y: f32| c + Vec2::new(x * h.cos() - y * h.sin(), x * h.sin() + y * h.cos());
        // Vecteur de route fond : 1 minute.
        if v.sog > 0.05 {
            let ahead = v.position.destination(v.cog, v.sog * 60.0);
            painter.arrow(
                c,
                self.to_screen(rect, ahead) - c,
                Stroke::new(2.0, Color32::from_rgb(120, 255, 120)),
            );
        }
        let head = rot(0.0, -60.0);
        painter.line_segment(
            [c, head],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 120)),
        );
        let hull = vec![
            rot(0.0, -16.0),
            rot(8.0, 10.0),
            rot(0.0, 6.0),
            rot(-8.0, 10.0),
        ];
        let fill = if v.anchored {
            Color32::from_rgb(160, 160, 160)
        } else {
            Color32::from_rgb(255, 210, 60)
        };
        painter.add(Shape::convex_polygon(
            hull,
            fill,
            Stroke::new(1.5, Color32::BLACK),
        ));

        // Échelle.
        let mpp = self.meters_per_pixel();
        let nm_steps = [
            0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0,
        ];
        let nm = nm_steps
            .iter()
            .copied()
            .find(|n| n * 1852.0 / mpp >= 80.0)
            .unwrap_or(500.0);
        let px = (nm * 1852.0 / mpp) as f32;
        let o = rect.right_bottom() - Vec2::new(20.0 + px, 22.0);
        painter.line_segment([o, o + Vec2::new(px, 0.0)], Stroke::new(3.0, text));
        painter.text(
            o + Vec2::new(px / 2.0, -4.0),
            Align2::CENTER_BOTTOM,
            format!("{nm} NM"),
            FontId::proportional(11.0),
            text,
        );
        painter.text(
            rect.left_top() + Vec2::new(6.0, 6.0),
            Align2::LEFT_TOP,
            format!(
                "z{:.1}  {}  SOG {:.1} kn",
                self.zoom,
                if self.follow { "suivi" } else { "libre" },
                mps_to_knots(v.sog)
            ),
            FontId::monospace(11.0),
            text,
        );
        if let Some(hp) = self.hover {
            painter.text(
                rect.left_top() + Vec2::new(6.0, 22.0),
                Align2::LEFT_TOP,
                format!("{}  {}", fmt_deg(hp.lat, true), fmt_deg(hp.lon, false)),
                FontId::monospace(11.0),
                text,
            );
        }

        if with_tiles {
            painter.text(
                rect.left_bottom() + Vec2::new(4.0, -30.0),
                Align2::LEFT_BOTTOM,
                "© OpenStreetMap contributors",
                FontId::proportional(10.0),
                Color32::from_gray(60),
            );
        }
        // Gestes.
        let pointer = resp
            .interact_pointer_pos()
            .map(|p| self.screen_to_geo(rect, p));
        let mods = ui.input(|i| i.modifiers);
        if resp.secondary_clicked() {
            if let Some(p) = pointer {
                return Some(if mods.shift {
                    MapAction::AddWaypoint(p)
                } else {
                    MapAction::Destination(p)
                });
            }
        }
        if resp.clicked() && mods.ctrl {
            if let Some(p) = pointer {
                return Some(MapAction::MoveVessel(p));
            }
        }
        if resp.double_clicked() {
            self.follow = true;
        }
        None
    }
}

/// Degrés décimaux → `35°01.234'S`.
pub fn fmt_deg(v: f64, lat: bool) -> String {
    let hemi = match (lat, v < 0.0) {
        (true, false) => 'N',
        (true, true) => 'S',
        (false, false) => 'E',
        (false, true) => 'W',
    };
    // Arrondi en millièmes de minute d'abord, pour reporter la retenue.
    let milli = (v.abs() * 60_000.0).round() as u64;
    format!(
        "{}°{:02}.{:03}'{}",
        milli / 60_000,
        (milli % 60_000) / 1000,
        milli % 1000,
        hemi
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_roundtrip() {
        let m = MapView::new(LatLon::new(-35.0, 138.5), 12.0, false);
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let p = LatLon::new(-35.01, 138.52);
        let back = m.screen_to_geo(rect, m.to_screen(rect, p));
        assert!((back.lat - p.lat).abs() < 1e-5 && (back.lon - p.lon).abs() < 1e-5);
        assert_eq!(m.screen_to_geo(rect, rect.center()).lat.round(), -35.0);
        assert_eq!(fmt_deg(-35.5, true), "35°30.000'S");
        assert_eq!(fmt_deg(-34.999_999_9, true), "35°00.000'S");
    }
}
