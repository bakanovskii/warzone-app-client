//! `+[EntityObject generateGUID]`, ARMv7 Thumb entry 0x36bb9.
//!
//! The original makes six arc4random calls, clears bits 10..31 of each result,
//! then formats NSString with `%d%d%d%d%d%d`. It adds neither separators nor
//! padding. Live ARM oracle evidence and adapter status are in port_manifest.json.

/// Format the six words in original call order, without consuming entropy.
///
/// Shadow execution must use words already consumed by the original call;
/// drawing six more would change the application's random stream.
pub fn entity_guid_from_words(words: [u32; 6]) -> String {
    let [a, b, c, d, e, f] = words.map(|word| word & 0x3ff);
    format!("{a}{b}{c}{d}{e}{f}")
}

/// Reconstruct the method with its entropy source supplied at the boundary.
///
/// This function does not seed or implement an RNG. A live adapter must supply
/// the same OS-backed arc4random service as the reference client, exactly once
/// per word. This legacy identifier format is not guaranteed collision-free.
pub fn generate_entity_guid(mut next_word: impl FnMut() -> u32) -> String {
    entity_guid_from_words(std::array::from_fn(|_| next_word()))
}
