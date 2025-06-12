use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use ort::Environment;
use rust_bert::pipelines::hf_tokenizers::HFTokenizer;
use rust_bert::pipelines::onnx::{config::ONNXEnvironmentConfig, ONNXEncoder};
use tch::{Kind, Tensor};

pub(crate) struct Embedder {
    encoder: ONNXEncoder,
}

impl Embedder {
    pub(crate) fn load_model(
        model_path: impl AsRef<Path>,
        environment: &Arc<Environment>,
    ) -> Result<Self> {
        let model_file = PathBuf::from(model_path.as_ref()).join("model.onnx");
        let onnx_config = ONNXEnvironmentConfig::default();
        let encoder = ONNXEncoder::new(model_file, environment, &onnx_config)
            .map_err(|e| anyhow::anyhow!("model loading failed: {}", e))?;

        Ok(Self { encoder })
    }

    pub(crate) fn create_environment() -> Result<Arc<Environment>> {
        Ok(Arc::new(
            Environment::builder().with_name("embedder").build()?,
        ))
    }

    pub(crate) fn encode_texts(&self, tokenizer: &HFTokenizer, texts: &[&str]) -> Result<Tensor> {
        let encoding = tokenizer
            .encode_list(texts)
            .map_err(|e| anyhow::anyhow!("tokenization failed: {}", e))?;

        let input_ids: Vec<_> = encoding.iter().map(|enc| enc.token_ids.clone()).collect();

        let attention_masks: Vec<_> = encoding
            .iter()
            .map(|enc| {
                enc.special_tokens_mask
                    .iter()
                    .map(|&m| i64::from(m))
                    .collect::<Vec<_>>()
            })
            .collect();

        let batch_size = i64::try_from(input_ids.len())
            .map_err(|_| anyhow::anyhow!("input_ids length exceeds i64 limits"))?;

        let input_tensor = Tensor::from_slice(&input_ids.concat())
            .view([batch_size, -1])
            .to_kind(Kind::Int64);

        let attention_mask = Tensor::from_slice(&attention_masks.concat())
            .view([batch_size, -1])
            .to_kind(Kind::Int64);

        let output =
            self.encoder
                .forward(Some(&input_tensor), Some(&attention_mask), None, None, None)?;

        output
            .last_hidden_state
            .ok_or_else(|| anyhow::anyhow!("empty last_hidden_state"))
    }

    /// Normalizes embeddings by L2 normalization.
    pub(crate) fn normalize_embeddings(embeddings: &Tensor) -> Tensor {
        let cls_embeddings = embeddings.select(1, 0);
        &cls_embeddings / cls_embeddings.norm_scalaropt_dim(2, [1], true)
    }
}

pub(crate) fn load_tokenizer(
    tokenizer_file: impl AsRef<Path>,
    special_token_map: impl AsRef<Path>,
) -> Result<HFTokenizer> {
    HFTokenizer::from_file(tokenizer_file, special_token_map).map_err(|e| anyhow::anyhow!(e))
}
