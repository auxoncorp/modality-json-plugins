use crate::config::AttrKeyRename;
use crate::error::Error;
use modality_api::{AttrKey, AttrVal, TimelineId};
use modality_ingest_client::dynamic::DynamicIngestClient;
use modality_ingest_client::{IngestClient, ReadyState};
use modality_ingest_protocol::InternedAttrKey;
use std::collections::{BTreeMap, HashMap};

pub struct Client {
    pub c: DynamicIngestClient,
    timeline_keys: BTreeMap<String, InternedAttrKey>,
    event_keys: BTreeMap<String, InternedAttrKey>,
    rename_timeline_attrs: HashMap<String, String>,
    rename_event_attrs: HashMap<String, String>,
    sent_timeline_attrs: HashMap<(TimelineId, InternedAttrKey), AttrVal>,
    current_timeline: Option<TimelineId>,
}

fn normalize_timeline_key(s: String) -> String {
    if s.starts_with("timeline.") {
        s
    } else {
        format!("timeline.{s}")
    }
}

fn normalize_event_key(s: String) -> String {
    if s.starts_with("event.") {
        s
    } else {
        format!("event.{s}")
    }
}

impl Client {
    pub fn new(
        c: IngestClient<ReadyState>,
        rename_timeline_attrs: Vec<AttrKeyRename>,
        rename_event_attrs: Vec<AttrKeyRename>,
    ) -> Self {
        Self {
            c: c.into(),
            timeline_keys: Default::default(),
            event_keys: Default::default(),
            rename_timeline_attrs: rename_timeline_attrs
                .into_iter()
                .map(|r| {
                    (
                        normalize_timeline_key(r.original),
                        normalize_timeline_key(r.new),
                    )
                })
                .collect(),
            rename_event_attrs: rename_event_attrs
                .into_iter()
                .map(|r| (normalize_event_key(r.original), normalize_event_key(r.new)))
                .collect(),
            sent_timeline_attrs: HashMap::new(),
            current_timeline: None,
        }
    }

    pub async fn send_event_on_timeline(
        &mut self,
        timeline_id: TimelineId,
        timeline_kvs: Vec<(AttrKey, AttrVal)>,
        ordering: u128,
        event_kvs: Vec<(AttrKey, AttrVal)>,
    ) -> Result<(), Error> {
        if self.current_timeline != Some(timeline_id) {
            self.c.open_timeline(timeline_id).await?;
            self.current_timeline = Some(timeline_id);
        }

        for (tk, tv) in timeline_kvs.into_iter() {
            let itk = self.interned_timeline_key(tk).await?;
            match self.sent_timeline_attrs.entry((timeline_id, itk)) {
                std::collections::hash_map::Entry::Occupied(mut ocupado) => {
                    if ocupado.get() != &tv {
                        self.c
                            .timeline_metadata(std::iter::once((itk, tv.clone())))
                            .await?;
                        ocupado.insert(tv);
                    }
                }
                std::collections::hash_map::Entry::Vacant(vaca) => {
                    self.c
                        .timeline_metadata(std::iter::once((itk, tv.clone())))
                        .await?;
                    vaca.insert(tv);
                }
            }
        }

        let mut interned_event_kvs = vec![];
        for (ek, ev) in event_kvs.into_iter() {
            interned_event_kvs.push((self.interned_event_key(ek).await?, ev));
        }

        self.c.event(ordering, interned_event_kvs).await?;

        Ok(())
    }

    async fn interned_timeline_key(&mut self, key: AttrKey) -> Result<InternedAttrKey, Error> {
        let mut key = &normalize_timeline_key(key.to_string());
        if let Some(new) = self.rename_timeline_attrs.get(key) {
            key = new;
        }

        let int_key = if let Some(k) = self.timeline_keys.get(key) {
            *k
        } else {
            let k = self.c.declare_attr_key(key.to_string()).await?;
            self.timeline_keys.insert(key.to_string(), k);
            k
        };
        Ok(int_key)
    }

    async fn interned_event_key(&mut self, key: AttrKey) -> Result<InternedAttrKey, Error> {
        let mut key = &normalize_event_key(key.to_string());
        if let Some(new) = self.rename_event_attrs.get(key) {
            key = new;
        }

        let int_key = if let Some(k) = self.event_keys.get(&key.to_string()) {
            *k
        } else {
            let k = self.c.declare_attr_key(key.to_string()).await?;
            self.event_keys.insert(key.to_string(), k);
            k
        };
        Ok(int_key)
    }
}
