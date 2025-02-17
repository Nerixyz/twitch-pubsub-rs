use rand::{distr::Alphanumeric, Rng};

pub fn generate_nonce<R: Rng>(rng: R) -> String {
    rng.sample_iter(&Alphanumeric)
        .take(30)
        .map(char::from)
        .collect::<String>()
}
