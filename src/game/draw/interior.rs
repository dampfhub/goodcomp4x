//! A city interior: its tactical grid, fighters, command post and gates.

use super::*;

pub(super) const POST_BAR_SCALE: f32 = 1.1;

impl GameState {
    /// The city interior is a separate world map, drawn with the normal hex
    /// and unit primitives rather than as buttons in a screen-space panel.
    pub(super) fn build_interior_vertices(&self, city: usize) -> Vec<Vertex> {
        let mut out = Vec::new();
        let city = &self.cities[city];
        let center_hex = Hex::new(0, 0);
        let selected = self.interior_selected.and_then(|source| {
            city.interior
                .fighters
                .iter()
                .find(|f| f.source_id == source)
        });
        for q in -2..=2 {
            for r in -2..=2 {
                let hex = Hex::new(q, r);
                if hex.distance(center_hex) > 2 {
                    continue;
                }
                let at = hex.to_world();
                mesh::regular_polygon(at, OUTER_BORDER_RADIUS, 6, 0.0, BORDER_COLOR, &mut out);
                let fill = if hex == center_hex {
                    if city.interior.core_hp > 0.0 {
                        [0.30, 0.24, 0.14, 1.0]
                    } else {
                        [0.19, 0.10, 0.09, 1.0]
                    }
                } else if hex.distance(center_hex) == 2 {
                    [0.21, 0.24, 0.27, 1.0]
                } else {
                    [0.26, 0.29, 0.31, 1.0]
                };
                mesh::regular_polygon(at, HEX_SIZE * HEX_FILL_SCALE, 6, 0.0, fill, &mut out);
                if let Some(fighter) = selected {
                    let occupied = city.interior.fighters.iter().find(|f| f.pos == hex);
                    let can_move = occupied.is_none()
                        && (hex != center_hex || city.interior.core_hp <= 0.0)
                        && fighter.move_reaches(hex);
                    let can_attack = (occupied.is_some_and(|other| other.team != fighter.team)
                        || (hex == center_hex
                            && city.team != fighter.team
                            && city.interior.core_hp > 0.0))
                        && fighter.attack_reaches(hex);
                    if can_move || can_attack {
                        mesh::polygon_outline(
                            at,
                            HEX_SIZE * 0.82,
                            0.065,
                            6,
                            0.0,
                            if can_attack {
                                ATTACK_RANGE_COLOR
                            } else {
                                MOVE_RANGE_COLOR
                            },
                            &mut out,
                        );
                    }
                }
            }
        }

        let post = center_hex.to_world();
        let breached = city.interior.core_hp <= 0.0;
        mesh::regular_polygon(post, 0.56, 6, 0.0, ICON_OUTLINE_COLOR, &mut out);
        mesh::regular_polygon(
            post,
            0.48,
            6,
            0.0,
            if breached {
                [0.22, 0.16, 0.14, 1.0]
            } else {
                city.team.color()
            },
            &mut out,
        );
        if breached {
            mesh::polygon_outline(post, 0.59, 0.07, 6, 0.0, SELECTED_COLOR, &mut out);
        }
        if breached {
            font::push_text_centered(
                post + Vec2::new(0.0, 0.13),
                0.16,
                "0 HP",
                SELECTED_COLOR,
                &mut out,
            );
            font::push_text_centered(
                post + Vec2::new(0.0, -0.14),
                if city.team == self.local_team {
                    0.17
                } else {
                    0.23
                },
                if city.team == self.local_team {
                    "DEFEND"
                } else {
                    "TAKE"
                },
                SELECTED_COLOR,
                &mut out,
            );
        } else {
            font::push_text_centered(
                post + Vec2::new(0.0, -0.07),
                0.23,
                "POST",
                LABEL_COLOR,
                &mut out,
            );
        }
        push_health_bar(
            post,
            city.interior.core_hp / crate::game::city::CORE_HP,
            POST_BAR_SCALE,
            &mut out,
        );

        for fighter in &city.interior.fighters {
            let at = fighter.pos.to_world();
            if self.interior_selected == Some(fighter.source_id) {
                mesh::polygon_outline(at, 0.72, 0.09, 6, 0.0, SELECTED_COLOR, &mut out);
            }
            if let Some(dest) = fighter.planned_move {
                push_unit_icon(
                    dest.to_world(),
                    UnitLook {
                        icon: UnitIcon::of(fighter.unit_type),
                        civilian: false,
                    },
                    0.85,
                    with_alpha(fighter.team.color(), GHOST_ALPHA),
                    &mut out,
                );
            }
            if let Some(target) = fighter.planned_attack {
                push_attack_arc(
                    fighter.planned_move.unwrap_or(fighter.pos).to_world(),
                    target.to_world(),
                    &mut out,
                );
            }
        }
        for fighter in &city.interior.fighters {
            let at = fighter.pos.to_world();
            push_unit_icon(
                at,
                UnitLook {
                    icon: UnitIcon::of(fighter.unit_type),
                    civilian: false,
                },
                1.0,
                fighter.team.color(),
                &mut out,
            );
            push_health_bar(at, fighter.health_fraction(), 1.0, &mut out);
        }
        if let Some(preview) = self.attack_preview() {
            let pulse = self.preview_pulse();
            for loss in &preview.losses {
                let (at, scale, max_hp) = match loss.hurt {
                    Hurt::Post => (post, POST_BAR_SCALE, crate::game::city::CORE_HP),
                    Hurt::Fighter(id) => {
                        let Some(fighter) =
                            city.interior.fighters.iter().find(|f| f.source_id == id)
                        else {
                            continue;
                        };
                        (fighter.pos.to_world(), 1.0, fighter.stats().max_hp)
                    }
                    _ => continue,
                };
                push_health_loss(at, loss, max_hp, scale, pulse, &mut out);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::terrain::Resource;
    use crate::game::unit::UnitType;

    #[test]
    fn upgraded_interior_fighter_highlight_click_and_health_agree() {
        let mut game = GameState::siege_scenario();
        let city = 1;
        let source = 0;
        game.interior_selected = Some(source);
        let fighter = game.cities[city]
            .interior
            .fighters
            .iter()
            .find(|f| f.source_id == source)
            .unwrap();
        assert_eq!(fighter.unit_type, UnitType::Melee);
        let origin = fighter.pos;
        let target = (-2..=2)
            .flat_map(|q| (-2..=2).map(move |r| Hex::new(q, r)))
            .find(|&hex| {
                hex.distance(Hex::new(0, 0)) <= 2
                    && hex != Hex::new(0, 0)
                    && origin.distance(hex) == 2
                    && game.cities[city]
                        .interior
                        .fighters
                        .iter()
                        .all(|other| other.pos != hex)
            })
            .expect("an open tile two steps away");
        let highlighted = |vertices: &[Vertex]| {
            let center = target.to_world();
            vertices.iter().any(|vertex| {
                vertex.color == MOVE_RANGE_COLOR
                    && Vec2::new(vertex.pos[0], vertex.pos[1]).distance(center) < HEX_SIZE * 0.85
            })
        };
        assert!(!highlighted(&game.build_interior_vertices(city)));
        let fighter = game.cities[city]
            .interior
            .fighters
            .iter_mut()
            .find(|f| f.source_id == source)
            .unwrap();
        fighter.training_upgrade = Some(Resource::Horses);
        assert!(fighter.move_reaches(target));
        assert!(highlighted(&game.build_interior_vertices(city)));
        game.interior_click(target);
        let fighter = game.cities[city]
            .interior
            .fighters
            .iter_mut()
            .find(|f| f.source_id == source)
            .unwrap();
        assert_eq!(fighter.planned_move, Some(target));

        fighter.training_upgrade = Some(Resource::Iron);
        let mut upgraded = UnitType::Melee.stats();
        crate::game::unit::apply_training_upgrade(&mut upgraded, Some(Resource::Iron));
        fighter.hp = upgraded.max_hp;
        assert_eq!(fighter.health_fraction(), 1.0);
        fighter.hp = UnitType::Melee.stats().max_hp;
        assert!(fighter.health_fraction() < 1.0);
    }
}
