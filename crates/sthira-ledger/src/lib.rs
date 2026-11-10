//! Kernel · the ledger that chains of anchors live on.
//!
//! [`AnchorLedger`] is the interface `sthira-node` uses; it is the seam a Canton-backed ledger
//! plugs into. [`SimLedger`] is an in-memory stand-in for Canton. It is not a blockchain, just
//! enough behaviour to tell the story:
//! - Every party runs a participant node that keeps its OWN copy of the contracts it is a stakeholder of.
//! - A chain belongs to its members. Only its submitter appends. What a chain anchors (its profile
//!   and contract template) is a label in its [`ChainSpec`]; the ledger behaves the same for all.
//! - An append succeeds only if EVERY member's node agrees it continues the chain (`seq` + `prevHead`),
//!   and the resulting anchor is signed by every member and stored on every member's node.
//! - Archiving (rewriting) an anchor needs every signatory, so no single party can do it.
//! - The synchronizer only orders encrypted envelopes: it sees recipients and sizes, never contents.
//!
//! A Canton-backed implementation (JSON Ledger API v2, Daml templates) will implement the same trait.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

pub type Party = String;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct LedgerError(pub String);

pub type Result<T> = std::result::Result<T, LedgerError>;

/// Everything fixed when a chain is created. Every member signs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChainSpec {
    pub chain_id: String,
    /// Contract template of the chain's anchors, e.g. `Sthiranet.Surveillance:SegmentAnchor`.
    pub template: String,
    /// The record profile the leaves follow, e.g. `surveillance/v1`.
    pub profile: String,
    /// The only party that may append.
    pub submitter: Party,
    pub members: Vec<Party>,
    pub genesis_head: String,
    /// Profile-specific facts the members agree to at genesis (a device's public key, a site…).
    pub attributes: BTreeMap<String, String>,
}

/// What the submitter sends when it appends a batch to its chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorInput {
    pub seq: u64,
    pub prev_head: String,
    pub head: String,
    pub merkle_root: String,
    pub event_count: u64,
    pub from_pos: String,
    pub to_pos: String,
    pub from_time: String,
    pub to_time: String,
}

/// An anchor contract as stored on a participant node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
    pub contract_id: String,
    pub template: String,
    pub chain_id: String,
    pub seq: u64,
    pub prev_head: String,
    pub head: String,
    pub merkle_root: String,
    pub event_count: u64,
    pub from_pos: String,
    pub to_pos: String,
    pub from_time: String,
    pub to_time: String,
    pub signatories: Vec<Party>,
    pub update_id: String,
    pub recorded_at: String,
}

/// One party's copy of a chain and its anchors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChainCopy {
    pub chain_id: String,
    pub template: String,
    pub profile: String,
    pub submitter: Party,
    pub members: Vec<Party>,
    pub genesis_head: String,
    pub attributes: BTreeMap<String, String>,
    pub last_seq: u64,
    pub last_head: String,
    pub anchors: Vec<Anchor>,
    pub created_update: String,
}

/// What the synchronizer sees of one transaction: who receives it and how big it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub update_id: String,
    pub at: String,
    pub recipients: Vec<Party>,
    pub bytes: usize,
    pub ciphertext: String,
}

/// The outcome of trying to archive (rewrite) an anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveAttempt {
    pub ok: bool,
    pub actor: Party,
    pub chain_id: String,
    pub seq: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,
    pub required: Vec<Party>,
    pub signed: Vec<Party>,
    pub missing: Vec<Party>,
    pub reason: String,
}

/// The ledger operations the node layer needs.
pub trait AnchorLedger: Send {
    fn add_party(&mut self, id: &str);
    /// Creates a chain signed by every member of `spec`.
    fn create_chain(&mut self, spec: &ChainSpec) -> Result<()>;
    /// Appends an anchor; every member's node must agree it continues the chain.
    fn append(&mut self, chain_id: &str, submitter: &str, input: AnchorInput) -> Result<Anchor>;
    /// Adds a member (needs every existing member's consent). The new member receives the history.
    fn add_member(&mut self, chain_id: &str, new_member: &str, submitter: &str) -> Result<()>;
    /// Tries to archive anchor `seq` with only `actor`'s signature.
    fn attempt_archive(&mut self, chain_id: &str, actor: &str, seq: u64) -> Result<ArchiveAttempt>;
    /// `party`'s copy of one chain, if it is a stakeholder.
    fn copy(&self, party: &str, chain_id: &str) -> Option<&ChainCopy>;
    /// Every chain on `party`'s node.
    fn view(&self, party: &str) -> Vec<ChainCopy>;
    /// The synchronizer's log, oldest first.
    fn envelopes(&self) -> &[Envelope];
}

const MAX_ENVELOPES: usize = 300;

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

fn rid(prefix: &str) -> String {
    format!("{prefix}{}", hex::encode(rand::random::<[u8; 5]>()))
}

/// In-memory stand-in for Canton. Each party's node holds independent copies.
#[derive(Debug, Default)]
pub struct SimLedger {
    /// party -> chain id -> that party's copy
    parties: HashMap<Party, BTreeMap<String, ChainCopy>>,
    envelopes: Vec<Envelope>,
}

impl SimLedger {
    pub fn new() -> Self {
        Self::default()
    }

    fn copy_mut(&mut self, party: &str, chain_id: &str) -> Option<&mut ChainCopy> {
        self.parties.get_mut(party)?.get_mut(chain_id)
    }

    fn sequence<T: Serialize>(&mut self, recipients: &[Party], payload: &T) -> Envelope {
        let size = serde_json::to_vec(payload).map(|v| v.len()).unwrap_or(0);
        let env = Envelope {
            update_id: rid("upd-"),
            at: now_iso(),
            recipients: recipients.to_vec(),
            bytes: size + 96 * recipients.len(),
            ciphertext: hex::encode(rand::random::<[u8; 12]>()),
        };
        self.envelopes.push(env.clone());
        if self.envelopes.len() > MAX_ENVELOPES {
            self.envelopes.remove(0);
        }
        env
    }
}

impl AnchorLedger for SimLedger {
    fn add_party(&mut self, id: &str) {
        self.parties.insert(id.to_string(), BTreeMap::new());
    }

    fn create_chain(&mut self, spec: &ChainSpec) -> Result<()> {
        if let Some(m) = spec.members.iter().find(|m| !self.parties.contains_key(*m)) {
            return Err(LedgerError(format!("Unknown party {m}")));
        }
        if !spec.members.contains(&spec.submitter) {
            return Err(LedgerError(format!(
                "Submitter {} is not a member of chain {}",
                spec.submitter, spec.chain_id
            )));
        }
        let env = self.sequence(&spec.members, spec);
        for m in &spec.members {
            let copy = ChainCopy {
                chain_id: spec.chain_id.clone(),
                template: spec.template.clone(),
                profile: spec.profile.clone(),
                submitter: spec.submitter.clone(),
                members: spec.members.clone(),
                genesis_head: spec.genesis_head.clone(),
                attributes: spec.attributes.clone(),
                last_seq: 0,
                last_head: spec.genesis_head.clone(),
                anchors: Vec::new(),
                created_update: env.update_id.clone(),
            };
            self.parties.get_mut(m).expect("checked above").insert(spec.chain_id.clone(), copy);
        }
        Ok(())
    }

    fn append(&mut self, chain_id: &str, submitter: &str, a: AnchorInput) -> Result<Anchor> {
        let own = self
            .copy(submitter, chain_id)
            .ok_or_else(|| LedgerError(format!("{submitter} is not a member of chain {chain_id}")))?;
        if own.submitter != submitter {
            return Err(LedgerError(format!("Only {} can append to chain {chain_id}", own.submitter)));
        }
        let members = own.members.clone();
        let template = own.template.clone();
        // Each member's node checks continuity against its own copy before it signs.
        for m in &members {
            let c =
                self.copy(m, chain_id).ok_or_else(|| LedgerError(format!("{m} holds no copy of chain {chain_id}")))?;
            if a.seq != c.last_seq + 1 {
                return Err(LedgerError(format!(
                    "{m} refused to sign: expected seq {}, got {}",
                    c.last_seq + 1,
                    a.seq
                )));
            }
            if a.prev_head != c.last_head {
                return Err(LedgerError(format!("{m} refused to sign: prev_head does not match its chain head")));
            }
        }
        let env = self.sequence(&members, &a);
        let anchor = Anchor {
            contract_id: rid("#00"),
            template,
            chain_id: chain_id.to_string(),
            seq: a.seq,
            prev_head: a.prev_head,
            head: a.head,
            merkle_root: a.merkle_root,
            event_count: a.event_count,
            from_pos: a.from_pos,
            to_pos: a.to_pos,
            from_time: a.from_time,
            to_time: a.to_time,
            signatories: members.clone(),
            update_id: env.update_id,
            recorded_at: env.at,
        };
        for m in &members {
            let c = self.copy_mut(m, chain_id).expect("checked above");
            c.anchors.push(anchor.clone());
            c.last_seq = anchor.seq;
            c.last_head = anchor.head.clone();
        }
        Ok(anchor)
    }

    fn add_member(&mut self, chain_id: &str, new_member: &str, submitter: &str) -> Result<()> {
        let src =
            self.copy(submitter, chain_id).ok_or_else(|| LedgerError(format!("Unknown chain {chain_id}")))?.clone();
        if src.members.iter().any(|m| m == new_member) {
            return Ok(());
        }
        if !self.parties.contains_key(new_member) {
            return Err(LedgerError(format!("Unknown party {new_member}")));
        }
        let mut members = src.members.clone();
        members.push(new_member.to_string());
        self.sequence(&members, &serde_json::json!({ "chainId": chain_id, "addMember": new_member }));
        for m in &src.members {
            if let Some(c) = self.copy_mut(m, chain_id) {
                c.members = members.clone();
            }
        }
        let mut copy = src;
        copy.members = members;
        self.parties.get_mut(new_member).expect("checked above").insert(chain_id.to_string(), copy);
        Ok(())
    }

    fn attempt_archive(&mut self, chain_id: &str, actor: &str, seq: u64) -> Result<ArchiveAttempt> {
        let Some(c) = self.copy(actor, chain_id) else {
            return Ok(ArchiveAttempt {
                ok: false,
                actor: actor.to_string(),
                chain_id: chain_id.to_string(),
                seq,
                contract_id: None,
                required: vec![],
                signed: vec![],
                missing: vec![],
                reason: format!(
                    "{actor}'s node holds no copy of chain {chain_id}: it is not a stakeholder, so it cannot even see these anchors."
                ),
            });
        };
        let anchor = c
            .anchors
            .iter()
            .find(|x| x.seq == seq)
            .ok_or_else(|| LedgerError(format!("Chain {chain_id} has no anchor #{seq}")))?;
        let required = anchor.signatories.clone();
        let contract_id = anchor.contract_id.clone();
        let signed = vec![actor.to_string()];
        let missing: Vec<Party> = required.iter().filter(|p| !signed.contains(p)).cloned().collect();
        self.sequence(&required, &serde_json::json!({ "archive": contract_id }));
        let reason = format!(
            "Archiving anchor #{seq} needs the signatures of {}. Only {actor} signed; {} did not. \
             The ledger rejected the transaction and the anchor is unchanged.",
            required.join(", "),
            missing.join(" and ")
        );
        Ok(ArchiveAttempt {
            ok: false,
            actor: actor.to_string(),
            chain_id: chain_id.to_string(),
            seq,
            contract_id: Some(contract_id),
            required,
            signed,
            missing,
            reason,
        })
    }

    fn copy(&self, party: &str, chain_id: &str) -> Option<&ChainCopy> {
        self.parties.get(party)?.get(chain_id)
    }

    fn view(&self, party: &str) -> Vec<ChainCopy> {
        self.parties.get(party).map(|c| c.values().cloned().collect()).unwrap_or_default()
    }

    fn envelopes(&self) -> &[Envelope] {
        &self.envelopes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> SimLedger {
        let mut l = SimLedger::new();
        for p in ["HQ", "S1", "S2", "AUDITOR"] {
            l.add_party(p);
        }
        l.create_chain(&ChainSpec {
            chain_id: "S1".into(),
            template: "Test:Anchor".into(),
            profile: "test/v1".into(),
            submitter: "S1".into(),
            members: vec!["S1".into(), "HQ".into()],
            genesis_head: "g".into(),
            attributes: BTreeMap::from([("k".to_string(), "v".to_string())]),
        })
        .unwrap();
        l
    }

    fn input(seq: u64, prev: &str, head: &str) -> AnchorInput {
        AnchorInput {
            seq,
            prev_head: prev.into(),
            head: head.into(),
            merkle_root: "r".into(),
            event_count: 0,
            from_pos: String::new(),
            to_pos: String::new(),
            from_time: String::new(),
            to_time: String::new(),
        }
    }

    #[test]
    fn append_is_cosigned_and_copied_to_every_member() {
        let mut l = setup();
        let a = l.append("S1", "S1", input(1, "g", "h1")).unwrap();
        assert_eq!(a.signatories, vec!["S1", "HQ"]);
        assert_eq!(a.template, "Test:Anchor");
        assert_eq!(l.copy("HQ", "S1").unwrap().last_head, "h1");
        assert_eq!(l.copy("HQ", "S1").unwrap().attributes["k"], "v");
        assert!(l.copy("S2", "S1").is_none());
    }

    #[test]
    fn fork_skip_and_wrong_submitter_are_refused() {
        let mut l = setup();
        l.append("S1", "S1", input(1, "g", "h1")).unwrap();
        assert!(l.append("S1", "S1", input(3, "h1", "h3")).unwrap_err().0.contains("expected seq 2"));
        assert!(l.append("S1", "S1", input(2, "g", "h2")).unwrap_err().0.contains("prev_head"));
        assert!(l.append("S1", "HQ", input(2, "h1", "h2")).unwrap_err().0.contains("Only S1"));
    }

    #[test]
    fn archive_needs_every_signatory() {
        let mut l = setup();
        l.append("S1", "S1", input(1, "g", "h1")).unwrap();
        assert_eq!(l.attempt_archive("S1", "S1", 1).unwrap().missing, vec!["HQ"]);
        assert_eq!(l.attempt_archive("S1", "HQ", 1).unwrap().missing, vec!["S1"]);
        assert!(l.attempt_archive("S1", "S2", 1).unwrap().required.is_empty());
        assert!(l.attempt_archive("S1", "S1", 9).is_err());
    }

    #[test]
    fn new_member_receives_history_and_cosigns() {
        let mut l = setup();
        l.append("S1", "S1", input(1, "g", "h1")).unwrap();
        l.add_member("S1", "AUDITOR", "S1").unwrap();
        assert_eq!(l.copy("AUDITOR", "S1").unwrap().anchors.len(), 1);
        let a = l.append("S1", "S1", input(2, "h1", "h2")).unwrap();
        assert_eq!(a.signatories, vec!["S1", "HQ", "AUDITOR"]);
    }

    #[test]
    fn synchronizer_log_is_capped() {
        let mut l = setup();
        let mut prev = "g".to_string();
        for s in 1..=MAX_ENVELOPES as u64 + 10 {
            let h = format!("h{s}");
            l.append("S1", "S1", input(s, &prev, &h)).unwrap();
            prev = h;
        }
        assert_eq!(l.envelopes().len(), MAX_ENVELOPES);
    }
}
