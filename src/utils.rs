use anyhow::Result;
use tch::Tensor;

pub(crate) fn tensor_to_vec(tensor: &Tensor) -> Result<Vec<f32>> {
    Vec::<f32>::try_from(tensor.squeeze()).map_err(|_| anyhow::anyhow!("Tensor conversion failed"))
}
