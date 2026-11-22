//! Who gets the next round of a recurring task (KONZEPT §7.1).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Points;

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Assignment {
    /// Whoever takes it first.
    Open,
    /// Always the same member.
    Fixed { member: Uuid },
    /// Members take turns; `next` is the index of the next one.
    Rotation { members: Vec<Uuid>, next: usize },
    /// The member with the fewest XP in the current period.
    LeastLoaded { members: Vec<Uuid> },
}

impl Assignment {
    pub fn members(&self) -> Vec<Uuid> {
        match self {
            Assignment::Open => vec![],
            Assignment::Fixed { member } => vec![*member],
            Assignment::Rotation { members, .. } | Assignment::LeastLoaded { members } => members.clone(),
        }
    }

    /// Picks the assignee for the next round. `load` = XP per member in the period.
    /// Returns the assignee and the updated assignment (rotation advances).
    /// Members that left the household (`active` = false) are skipped.
    pub fn pick(&self, load: &[(Uuid, Points)], active: &dyn Fn(Uuid) -> bool) -> (Option<Uuid>, Assignment) {
        match self {
            Assignment::Open => (None, self.clone()),
            Assignment::Fixed { member } => (active(*member).then_some(*member), self.clone()),
            Assignment::Rotation { members, next } => {
                let n = members.len();
                for offset in 0..n {
                    let i = (next + offset) % n;
                    if active(members[i]) {
                        return (Some(members[i]), Assignment::Rotation { members: members.clone(), next: (i + 1) % n });
                    }
                }
                (None, self.clone())
            }
            Assignment::LeastLoaded { members } => {
                let xp = |m: &Uuid| load.iter().find(|(id, _)| id == m).map(|(_, p)| *p).unwrap_or(0);
                // min_by_key keeps the first of equal minima, so ties go to the earlier member.
                let pick = members.iter().filter(|m| active(**m)).min_by_key(|m| xp(m)).copied();
                (pick, self.clone())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (Uuid, Uuid, Uuid) {
        (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3))
    }

    #[test]
    fn rotation_takes_turns_and_skips_inactive() {
        let (a, b, c) = ids();
        let r = Assignment::Rotation { members: vec![a, b, c], next: 0 };
        let all = |_: Uuid| true;
        let (p1, r) = r.pick(&[], &all);
        let (p2, r) = r.pick(&[], &all);
        let (p3, r) = r.pick(&[], &all);
        let (p4, r) = r.pick(&[], &all);
        assert_eq!([p1, p2, p3, p4], [Some(a), Some(b), Some(c), Some(a)]);
        let without_b = move |m: Uuid| m != b;
        let (p5, _) = r.pick(&[], &without_b);
        assert_eq!(p5, Some(c), "b left the household");
    }

    #[test]
    fn least_loaded_and_ties() {
        let (a, b, c) = ids();
        let l = Assignment::LeastLoaded { members: vec![a, b, c] };
        let all = |_: Uuid| true;
        assert_eq!(l.pick(&[(a, 50), (b, 20), (c, 30)], &all).0, Some(b));
        assert_eq!(l.pick(&[(a, 10)], &all).0, Some(b), "no XP yet counts as 0, first of ties wins");
    }

    #[test]
    fn fixed_and_open() {
        let (a, _, _) = ids();
        assert_eq!(Assignment::Fixed { member: a }.pick(&[], &|_| true).0, Some(a));
        assert_eq!(Assignment::Fixed { member: a }.pick(&[], &|_| false).0, None);
        assert_eq!(Assignment::Open.pick(&[], &|_| true).0, None);
        let json = serde_json::to_string(&Assignment::Rotation { members: vec![a], next: 0 }).unwrap();
        assert!(json.starts_with(r#"{"type":"rotation""#));
    }
}
