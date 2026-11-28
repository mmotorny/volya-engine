// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The items the stalkers show on their bodies
//! (docs/behavior/character-items.md section 8, cited as CH-6; the guitar
//! of its worked example, section 11): each stalker's list
//! `attachable_items` (8.1), the items it carries that may hang on it, each
//! with its offset *O* and bone (8.2), which holder accepts an item (8.3),
//! when it is attached and detached (8.4) and where it is drawn (8.5):
//! *O* · *B* · *X*, *B* the holder's bone as last posed, *X* the holder's
//! transform, as its own model.
//!
//! The level loads the models of the items its stalkers carry at the start
//! whose sections they list ([`carried_items`]), as instances of their own
//! not drawn until attached; an item made in play has no model to show (our
//! limitation, as the level's objects: visuals load with the level). The
//! torch is left out: it is drawn through its guide bone (torch.md 7), and
//! characters' torches are TODO(#223). The trader accepts nothing and the
//! player's drawing is not CH-6's, so only stalkers hold items here.
//! TODO(#1238): the corpse-model flag (8.3), which needs the death time,
//! is taken as clear.

use std::collections::{BTreeMap, HashMap};

use crate::core::character::inventory::Place;
use crate::core::config::{Config, value};
use crate::core::simulation::Simulation;
use crate::core::torch::{Mat4, attach_offset, multiply};
use crate::objects::LevelObject;
use volya_engine_formats::object::Kind;

/// The class id of the torch (CH-6 1).
const TORCH_CLASS: &str = "TORCH_S";

/// A holder's list (8.1): `text` split at commas, each item with the
/// characters up to and including space removed from both ends; an empty
/// text gives an empty list.
pub fn list(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let trim = |c: char| c <= ' ';
    text.split(',')
        .map(|s| s.trim_matches(trim).to_owned())
        .collect()
}

/// Section `section`'s `attachable_items` (8.1): an absent key gives an
/// empty list.
pub fn holder_list(system: &Config, section: &str) -> Vec<String> {
    let text = system.get(section, "attachable_items").flatten();
    text.map(list).unwrap_or_default()
}

/// What an item section gives its attachment (8.2), for a section with
/// `attach_angle_offset`: its offset *O*, its bone and its model.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemSection {
    pub offset: Mat4,
    /// `attach_bone_name`, lowercase.
    pub bone: String,
    /// The section's `visual` (8.5).
    pub visual: String,
}

impl ItemSection {
    /// Item section `section`'s, if it has `attach_angle_offset` and a
    /// `visual`. A missing `attach_position_offset` or `attach_bone_name`
    /// is read as zeros and no bone (our choice: every section with the
    /// angles has both, CH-6 1).
    pub fn read(system: &Config, section: &str) -> Option<Self> {
        let s = system.section(section)?;
        let angles = s.get("attach_angle_offset").flatten()?;
        let position = s.get("attach_position_offset").flatten().unwrap_or("");
        let bone = s.get("attach_bone_name").flatten().unwrap_or("");
        let visual = value::unquote(s.get("visual").flatten()?.trim());
        if visual.is_empty() {
            return None;
        }
        Some(Self {
            offset: attach_offset(value::floats::<3>(angles), value::floats::<3>(position)),
            bone: value::unquote(bone.trim()).to_ascii_lowercase(),
            visual: visual.to_owned(),
        })
    }
}

/// The items the level loads the models of (see the module): every item
/// a stalker of `holders` carries at the start whose section it lists and
/// whose section [`ItemSection::read`] reads, but torches; each with its
/// section's model as its visual, not online (drawn only when attached).
pub fn carried_items(
    simulation: &Simulation,
    system: &Config,
    holders: &[LevelObject],
) -> Vec<(LevelObject, ItemSection)> {
    let mut lists: HashMap<u16, Vec<String>> = HashMap::new();
    for h in holders.iter().filter(|h| h.record.kind() == Kind::Stalker) {
        let list = holder_list(system, h.record.header.section.as_str());
        if !list.is_empty() {
            lists.insert(h.id, list);
        }
    }
    let mut sections: HashMap<String, Option<ItemSection>> = HashMap::new();
    let mut items = Vec::new();
    for o in simulation.objects() {
        let Some(list) = lists.get(&o.record.header.parent_id) else {
            continue;
        };
        let section = o.record.header.section.as_str();
        if o.record.class_id.as_str() == TORCH_CLASS || !list.iter().any(|s| s == section) {
            continue;
        }
        let read = sections.entry(section.to_owned());
        let Some(item) = read.or_insert_with(|| ItemSection::read(system, section)) else {
            continue;
        };
        let object = LevelObject {
            id: o.id,
            index: o.spawn_index.unwrap_or(u32::from(o.id)),
            online: false,
            record: o.record.clone(),
            visual: Some(item.visual.clone()),
        };
        items.push((object, item.clone()));
    }
    items
}

/// An item that can show on its holder: its instance and section.
#[derive(Clone, Debug)]
pub struct ShownItem {
    pub instance: usize,
    pub section: String,
    pub attachment: ItemSection,
}

/// A stalker that holds items: its instance and list (8.1).
#[derive(Clone, Debug)]
pub struct Holder {
    pub instance: usize,
    pub list: Vec<String>,
}

/// What the holder's inventory says of an item, as the scripts keep it
/// (script API S).
pub trait Holdings {
    /// The holder whose inventory holds `item`, and its place there.
    fn held(&self, item: u16) -> Option<(u16, Place)>;
    /// `item`'s enabled flag (8.2).
    fn enabled(&self, item: u16) -> bool;
    /// `item`'s dropped mark (S 5.4).
    fn dropped(&self, item: u16) -> bool;
    /// Clears `item`'s enabled flag: it has left its holder (8.4).
    fn disable(&mut self, item: u16);
}

/// An attached item: its id and its holder's bone (`None`: the holder's
/// model lacks it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Attached {
    pub item: u16,
    pub bone: Option<usize>,
}

/// The stalkers' items shown on them.
#[derive(Debug, Default)]
pub struct AttachedItems {
    items: HashMap<u16, ShownItem>,
    holders: HashMap<u16, Holder>,
    /// By holder, in the order attached (8.5).
    attached: BTreeMap<u16, Vec<Attached>>,
    /// Each item's holder and place as last seen.
    seen: HashMap<u16, (u16, Place)>,
}

impl AttachedItems {
    pub fn new(items: HashMap<u16, ShownItem>, holders: HashMap<u16, Holder>) -> Self {
        Self {
            items,
            holders,
            ..Default::default()
        }
    }

    /// The items that can show, by id.
    pub fn items(&self) -> &HashMap<u16, ShownItem> {
        &self.items
    }

    /// Whether `id` is an item that can show.
    pub fn holds(&self, id: u16) -> bool {
        self.items.contains_key(&id)
    }

    /// Holder `holder`'s attached items, in order.
    pub fn attached(&self, holder: u16) -> &[Attached] {
        self.attached.get(&holder).map_or(&[], Vec::as_slice)
    }

    /// Whether `holder` accepts `item` (8.3, a stalker; the corpse-model
    /// flag taken as clear): the item is enabled, held by it (its own
    /// test: a character's belt does not count, so any place), its section
    /// is in the holder's list, and no item of the same section is
    /// attached (one with the dropped mark is not counted).
    fn accepts(&self, holder: u16, item: u16, holdings: &dyn Holdings) -> bool {
        let (Some(h), Some(i)) = (self.holders.get(&holder), self.items.get(&item)) else {
            return false;
        };
        let same = |a: &Attached| {
            (self.items.get(&a.item)).is_some_and(|o| o.section == i.section)
                && !holdings.dropped(a.item)
        };
        holdings.enabled(item)
            && holdings.held(item).is_some_and(|(by, _)| by == holder)
            && h.list.contains(&i.section)
            && !self.attached(holder).iter().any(same)
    }

    /// The holder tries to attach `item` (8.4): left alone when attached;
    /// otherwise, if it accepts it, its bone looked up by `bone` (in the
    /// holder's instance's model) and the item appended.
    fn try_attach(
        &mut self,
        holder: u16,
        item: u16,
        holdings: &dyn Holdings,
        bone: &dyn Fn(usize, &str) -> Option<usize>,
    ) {
        if self.attached(holder).iter().any(|a| a.item == item)
            || !self.accepts(holder, item, holdings)
        {
            return;
        }
        let (h, i) = (&self.holders[&holder], &self.items[&item]);
        let bone = bone(h.instance, &i.attachment.bone);
        let list = self.attached.entry(holder).or_default();
        list.push(Attached { item, bone });
    }

    /// Detaches `item` from `holder` (8.4), if attached.
    fn detach(&mut self, holder: u16, item: u16) {
        if let Some(list) = self.attached.get_mut(&holder) {
            list.retain(|a| a.item != item);
            if list.is_empty() {
                self.attached.remove(&holder);
            }
        }
    }

    /// `enable_attachable_item` (8.4, S 8.1), its flag already set or
    /// cleared: `attached` tries to attach `item` to `holder`, otherwise it
    /// is detached.
    pub fn enable(
        &mut self,
        item: u16,
        holder: u16,
        attached: bool,
        holdings: &dyn Holdings,
        bone: &dyn Fn(usize, &str) -> Option<usize>,
    ) {
        if attached {
            self.try_attach(holder, item, holdings, bone);
        } else {
            self.detach(holder, item);
        }
    }

    /// Follows the inventories (8.4): an item that left its holder is
    /// detached and its flag cleared; one taken, or put into a slot, is
    /// tried; one moved to the backpack is detached. Putting it on the belt
    /// changes nothing.
    pub fn follow(
        &mut self,
        holdings: &mut dyn Holdings,
        bone: &dyn Fn(usize, &str) -> Option<usize>,
    ) {
        let mut ids: Vec<u16> = self.items.keys().copied().collect();
        ids.sort_unstable();
        for item in ids {
            let now = holdings.held(item);
            let was = self.seen.get(&item).copied();
            if now == was {
                continue;
            }
            match now {
                Some(n) => self.seen.insert(item, n),
                None => self.seen.remove(&item),
            };
            if let Some((old, _)) = was
                && now.is_none_or(|(h, _)| h != old)
            {
                self.detach(old, item);
                holdings.disable(item);
            }
            let Some((holder, place)) = now else { continue };
            let taken = was.is_none_or(|(h, _)| h != holder);
            match place {
                Place::Backpack if !taken => self.detach(holder, item),
                Place::Slot(_) => self.try_attach(holder, item, holdings, bone),
                _ if taken => self.try_attach(holder, item, holdings, bone),
                _ => {}
            }
        }
    }

    /// Holder `holder` went offline or was removed: its items go with it,
    /// detached (believed, 8.6).
    pub fn holder_gone(&mut self, holder: u16) {
        self.attached.remove(&holder);
    }

    /// Each item's drawn transform (8.5), by its instance: *O* · *B* ·
    /// *X* for an attached item whose holder is drawn at *X* (`placement`
    /// of its instance) and whose bone is found, *B* that bone's
    /// model-space matrix as `pose` gives it; `None` for the others.
    pub fn placements(
        &self,
        placement: &dyn Fn(usize) -> Option<Mat4>,
        pose: &dyn Fn(usize, usize) -> Mat4,
    ) -> Vec<(usize, Option<Mat4>)> {
        let mut shown: HashMap<usize, Mat4> = HashMap::new();
        for (holder, list) in &self.attached {
            let h = &self.holders[holder];
            let Some(x) = placement(h.instance) else {
                continue;
            };
            for a in list {
                let (Some(bone), Some(i)) = (a.bone, self.items.get(&a.item)) else {
                    continue;
                };
                let b = pose(h.instance, bone);
                shown.insert(
                    i.instance,
                    multiply(&multiply(&i.attachment.offset, &b), &x),
                );
            }
        }
        let mut all: Vec<(usize, Option<Mat4>)> = (self.items.values())
            .map(|i| (i.instance, shown.get(&i.instance).copied()))
            .collect();
        all.sort_unstable_by_key(|(k, _)| *k);
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An item's holder and place, enabled flag and dropped mark.
    type Entry = (Option<(u16, Place)>, bool, bool);

    /// Holdings over a table of entries by item.
    #[derive(Default)]
    struct Table(HashMap<u16, Entry>);

    impl Holdings for Table {
        fn held(&self, item: u16) -> Option<(u16, Place)> {
            self.0.get(&item).and_then(|e| e.0)
        }
        fn enabled(&self, item: u16) -> bool {
            self.0.get(&item).is_some_and(|e| e.1)
        }
        fn dropped(&self, item: u16) -> bool {
            self.0.get(&item).is_some_and(|e| e.2)
        }
        fn disable(&mut self, item: u16) {
            if let Some(e) = self.0.get_mut(&item) {
                e.1 = false;
            }
        }
    }

    const GUITAR: u16 = 10;
    const GUITAR_2: u16 = 11;
    const BREAD: u16 = 12;
    const NOVICE: u16 = 1;
    const TRADER: u16 = 2;

    fn guitar() -> ItemSection {
        ItemSection {
            offset: attach_offset(
                [-1.651084, -1.341824, 1.200789],
                [0.077492, -0.030020, 0.090059],
            ),
            bone: "bip01_l_hand".into(),
            visual: "equipments\\item_guitar".into(),
        }
    }

    fn items() -> AttachedItems {
        let shown = |instance, section: &str, attachment| ShownItem {
            instance,
            section: section.into(),
            attachment,
        };
        let bread = ItemSection {
            bone: "bip01_r_hand".into(),
            ..guitar()
        };
        let items = HashMap::from([
            (GUITAR, shown(5, "guitar_a", guitar())),
            (GUITAR_2, shown(6, "guitar_a", guitar())),
            (BREAD, shown(7, "bread", bread)),
        ]);
        let holders = HashMap::from([(
            NOVICE,
            Holder {
                instance: 0,
                list: list("device_torch,attachable_item,hand_radio,bread, vodka, guitar_a"),
            },
        )]);
        AttachedItems::new(items, holders)
    }

    /// The left hand is bone 3, the right hand bone 4.
    fn bone(instance: usize, name: &str) -> Option<usize> {
        assert_eq!(instance, 0);
        match name {
            "bip01_l_hand" => Some(3),
            "bip01_r_hand" => Some(4),
            _ => None,
        }
    }

    fn translation(t: [f32; 3]) -> Mat4 {
        let mut m = volya_engine_formats::model::transform([0.0; 3], [0.0; 3]);
        m[3] = [t[0], t[1], t[2], 1.0];
        m
    }

    #[test]
    fn lists_are_split_and_trimmed() {
        assert_eq!(list("a,b, c ,\td"), ["a", "b", "c", "d"]);
        assert!(list("").is_empty());
        assert_eq!(list(" "), [""]);
    }

    #[test]
    fn the_guitar_follows_the_left_hand() {
        let mut a = items();
        let mut t = Table::default();
        // Taken into the backpack, disabled (CH-6 11 step 1): not shown.
        t.0.insert(GUITAR, (Some((NOVICE, Place::Backpack)), false, false));
        a.follow(&mut t, &bone);
        assert!(a.attached(NOVICE).is_empty());
        // Enabled by the script (step 3): attached from the backpack.
        t.0.get_mut(&GUITAR).unwrap().1 = true;
        a.enable(GUITAR, NOVICE, true, &t, &bone);
        assert_eq!(
            a.attached(NOVICE),
            [Attached {
                item: GUITAR,
                bone: Some(3)
            }]
        );
        // Drawn at O · B · X, following the bone as it moves.
        let x = translation([10.0, 0.0, 0.0]);
        let mut b = translation([0.0, 1.0, 0.0]);
        for _ in 0..2 {
            let placed = a.placements(&|k| (k == 0).then_some(x), &|_, bone| {
                assert_eq!(bone, 3);
                b
            });
            let want = multiply(&multiply(&guitar().offset, &b), &x);
            assert_eq!(placed, [(5, Some(want)), (6, None), (7, None)]);
            b = translation([0.0, 1.5, 0.2]);
        }
        // Not drawn while its holder is not.
        let placed = a.placements(&|_| None, &|_, _| b);
        assert!(placed.iter().all(|(_, p)| p.is_none()));
        // Disabled by the script (step 5): hidden again.
        t.0.get_mut(&GUITAR).unwrap().1 = false;
        a.enable(GUITAR, NOVICE, false, &t, &bone);
        assert!(a.attached(NOVICE).is_empty());
    }

    #[test]
    fn one_item_of_a_section_and_only_what_the_list_names() {
        let mut a = items();
        let mut t = Table::default();
        for id in [GUITAR, GUITAR_2, BREAD] {
            t.0.insert(id, (Some((NOVICE, Place::Backpack)), true, false));
        }
        a.follow(&mut t, &bone);
        // Taken enabled: tried at the take.
        let ids: Vec<u16> = a.attached(NOVICE).iter().map(|a| a.item).collect();
        assert_eq!(ids, [GUITAR, BREAD]);
        // A second guitar waits for the first to be dropped-marked.
        a.enable(GUITAR_2, NOVICE, true, &t, &bone);
        assert_eq!(a.attached(NOVICE).len(), 2);
        t.0.get_mut(&GUITAR).unwrap().2 = true;
        a.enable(GUITAR_2, NOVICE, true, &t, &bone);
        assert_eq!(a.attached(NOVICE).len(), 3);
        // The trader, with no list, accepts nothing.
        t.0.insert(BREAD, (Some((TRADER, Place::Backpack)), true, false));
        a.enable(BREAD, TRADER, true, &t, &bone);
        assert!(a.attached(TRADER).is_empty());
    }

    #[test]
    fn leaving_moving_and_going_offline_detach() {
        let mut a = items();
        let mut t = Table::default();
        t.0.insert(GUITAR, (Some((NOVICE, Place::None)), true, false));
        a.follow(&mut t, &bone);
        assert_eq!(a.attached(NOVICE).len(), 1);
        // Into the backpack: detached; into a slot: attached again; onto
        // the belt: unchanged.
        t.0.get_mut(&GUITAR).unwrap().0 = Some((NOVICE, Place::Backpack));
        a.follow(&mut t, &bone);
        assert!(a.attached(NOVICE).is_empty());
        t.0.get_mut(&GUITAR).unwrap().0 = Some((NOVICE, Place::Slot(2)));
        a.follow(&mut t, &bone);
        assert_eq!(a.attached(NOVICE).len(), 1);
        t.0.get_mut(&GUITAR).unwrap().0 = Some((NOVICE, Place::Belt));
        a.follow(&mut t, &bone);
        assert_eq!(a.attached(NOVICE).len(), 1);
        // Leaving the inventory: detached, its flag cleared.
        t.0.get_mut(&GUITAR).unwrap().0 = None;
        a.follow(&mut t, &bone);
        assert!(a.attached(NOVICE).is_empty());
        assert!(!t.enabled(GUITAR));
        // The holder going offline takes them all.
        t.0.insert(GUITAR, (Some((NOVICE, Place::Backpack)), true, false));
        a.follow(&mut t, &bone);
        assert_eq!(a.attached(NOVICE).len(), 1);
        a.holder_gone(NOVICE);
        assert!(a.attached(NOVICE).is_empty());
    }

    #[test]
    fn a_bone_the_model_lacks_is_attached_but_not_drawn() {
        let mut a = items();
        a.holders
            .get_mut(&NOVICE)
            .unwrap()
            .list
            .push("vodka".into());
        a.items.get_mut(&BREAD).unwrap().attachment.bone = "no_bone".into();
        let mut t = Table::default();
        t.0.insert(BREAD, (Some((NOVICE, Place::Backpack)), true, false));
        a.follow(&mut t, &bone);
        assert_eq!(a.attached(NOVICE)[0].bone, None);
        let x = translation([1.0, 2.0, 3.0]);
        let placed = a.placements(&|_| Some(x), &|_, _| x);
        assert!(placed.iter().all(|(_, p)| p.is_none()));
    }
}
