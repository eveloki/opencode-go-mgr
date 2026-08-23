//! Simple obfuscation for API keys.
//!
//! This is intentionally lightweight: keys are not stored in plain text on disk,
//! but the scheme is NOT a substitute for a real KMS or AES-GCM. If stronger
//! security is needed later, replace this module with `ring`/`aes-gcm`.
//!
//! Two cipher implementations are provided:
//! - `MachineBoundCipher`: derives a key from Windows environment variables
//!   (USERNAME, COMPUTERNAME, APPDATA) for backward compatibility with the
//!   original GUI app.
//! - `StaticKeyCipher`: derives a key from an arbitrary user-supplied secret,
//!   suitable for headless / cross-platform / Docker deployments.

#[doc(inline)]
pub use ocg_infra::crypto::{
    KeyCipher, MachineBoundCipher, StaticKeyCipher, load_or_create_static_cipher,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ocg-crypto-facade-{name}-{}", uuid::Uuid::new_v4()))
    }

    fn assert_same_type<T>(_: &T, _: &T) {}

    #[test]
    fn facade_reexports_infra_cipher_types() {
        let machine = MachineBoundCipher::new();
        let static_cipher = StaticKeyCipher::new("my-secret-key");
        assert_same_type(&machine, &ocg_infra::crypto::MachineBoundCipher::new());
        assert_same_type(
            &static_cipher,
            &ocg_infra::crypto::StaticKeyCipher::new("k"),
        );
    }

    #[test]
    fn machine_bound_roundtrip() {
        let original = "sk-ocg-test-key-12345";
        let cipher = MachineBoundCipher::new();
        let encrypted = cipher.encrypt(original).unwrap();
        assert_ne!(encrypted, original);
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn static_key_roundtrip() {
        let original = "sk-ocg-test-key-12345";
        let cipher = StaticKeyCipher::new("my-secret-key");
        let encrypted = cipher.encrypt(original).unwrap();
        assert_ne!(encrypted, original);
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn static_key_empty_string_roundtrip() {
        let cipher = StaticKeyCipher::new("k");
        let enc = cipher.encrypt("").unwrap();
        assert_eq!(enc, "");
        let dec = cipher.decrypt("").unwrap();
        assert_eq!(dec, "");
    }

    #[test]
    fn static_key_wrong_secret_fails_to_decrypt() {
        let enc = StaticKeyCipher::new("right-key")
            .encrypt("payload")
            .unwrap();
        let result = StaticKeyCipher::new("wrong-key").decrypt(&enc);
        match result {
            Err(_) => {}
            Ok(s) => assert_ne!(s, "payload"),
        }
    }

    #[test]
    fn static_key_file_is_created_and_reused() {
        let dir = test_dir("reuse");
        let first = load_or_create_static_cipher(&dir).unwrap();
        let key_path = dir.join(".encryption-key");
        let original = fs::read_to_string(&key_path).unwrap();
        let second = load_or_create_static_cipher(&dir).unwrap();

        assert!(!original.is_empty());
        assert_eq!(fs::read_to_string(&key_path).unwrap(), original);
        assert_eq!(
            second.decrypt(&first.encrypt("payload").unwrap()).unwrap(),
            "payload"
        );

        fs::remove_dir_all(dir).unwrap();
    }
}
