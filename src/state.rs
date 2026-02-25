use crate::config::Config;
use crate::input::{OneHotVocab, TfIdfVocab, TokenIndexVocab};
use crate::models::ModelRegistry;
use crate::receipt::ReceiptStore;

use ark_bn254::Fr;
use jolt_core::poly::commitment::dory::DoryCommitmentScheme;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use zkml_jolt_core::jolt::{JoltProverPreprocessing, JoltVerifierPreprocessing};

type PCS = DoryCommitmentScheme;

pub struct PreprocessingCache {
    pub prover: JoltProverPreprocessing<Fr, PCS>,
    pub verifier: JoltVerifierPreprocessing<Fr, PCS>,
}

// SAFETY: The preprocessing data is read-only after construction.
// JoltProverPreprocessing/JoltVerifierPreprocessing contain arkworks types
// that are safe to share across threads but don't implement Send/Sync.
unsafe impl Send for PreprocessingCache {}
unsafe impl Sync for PreprocessingCache {}

pub enum VocabData {
    TfIdf(TfIdfVocab),
    OneHot(OneHotVocab),
    TokenIndex(TokenIndexVocab),
}

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub receipts: ReceiptStore,
    pub registry: Arc<RwLock<ModelRegistry>>,
    pub vocabs: Arc<HashMap<String, VocabData>>,
    pub preprocessing: Arc<dashmap::DashMap<String, PreprocessingCache>>,
}
