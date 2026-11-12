//! Minimal end-to-end check of the public borrowed-Icf Python wrapper.
use super::Icf;
use pyo3::exceptions::PyReferenceError;
use pyo3::prelude::*;

#[test]
fn channel_write_is_readonly_live_and_expires() {
    Python::initialize();
    Python::attach(|py| {
        let event = cs705_core::icf::record::ChannelWrite::Body {
            stage: cs705_core::icf::record::ChannelStage::Frequency,
            offset: 7,
            bytes: b"ab",
        };
        let (retained, bytes) = super::record::with_channel_write(py, &event, |view| {
            assert_eq!(
                view.bind(py).getattr("variant")?.extract::<String>()?,
                "Body"
            );
            assert_eq!(view.bind(py).getattr("offset")?.extract::<usize>()?, 7);
            let bytes = view.bind(py).getattr("bytes")?;
            assert_eq!(bytes.call_method0("to_bytes")?.extract::<Vec<u8>>()?, b"ab");
            assert!(bytes.set_item(0, 1).is_err());
            Ok((view, bytes.unbind()))
        })
        .unwrap();
        for name in ["variant", "offset", "bytes", "byte"] {
            assert!(
                retained
                    .bind(py)
                    .getattr(name)
                    .unwrap_err()
                    .is_instance_of::<PyReferenceError>(py)
            );
        }
        assert!(
            retained
                .bind(py)
                .call_method0("stage")
                .unwrap_err()
                .is_instance_of::<PyReferenceError>(py)
        );
        assert!(
            bytes
                .bind(py)
                .call_method0("to_bytes")
                .unwrap_err()
                .is_instance_of::<PyReferenceError>(py)
        );
    });
}

#[test]
fn context_take_blocks_python_access_and_restores_updates() {
    Python::initialize();
    Python::attach(|py| {
        let context =
            Py::new(
                py,
                super::context::Context::from_core(
                    cs705_core::icf::context::Context::before_registry(),
                ),
            )
            .unwrap();
        let mut native = context.get().take().unwrap();
        assert!(
            context
                .bind(py)
                .call_method0("as_bytes")
                .unwrap_err()
                .is_instance_of::<pyo3::exceptions::PyRuntimeError>(py)
        );
        py.detach(move || {
            native.decode_packed(0);
        });
        assert!(context.bind(py).call_method0("as_bytes").is_ok());
    });
}

#[test]
fn metadata_projection_keeps_private_state_and_live_nested_fields() {
    Python::initialize();
    Python::attach(|py| {
        let native = cs705_core::icf::settings::gps::choice_fields()[0].clone();
        let original_key = native.setting.key;
        let root = crate::clone::Handle::new(native);
        let setting = Py::new(
            py,
            super::settings::Setting::from_projection(root.project(|v| Ok(&mut v.setting))),
        )
        .unwrap();
        setting
            .bind(py)
            .setattr("name", "custom GPS label")
            .unwrap();
        setting
            .bind(py)
            .setattr("feature_id", "custom feature")
            .unwrap();
        root.with(|v| {
            assert_eq!(v.setting.name, "custom GPS label");
            assert_eq!(v.setting.feature_id, "custom feature");
            assert_eq!(v.setting.key, original_key);
            Ok(())
        })
        .unwrap();
        let domain = setting.bind(py).getattr("domain").unwrap();
        domain.setattr("min", 2).unwrap();
        assert_eq!(root.with(|v| Ok(v.setting.domain.min)).unwrap(), 2);
        let replacement = Py::new(
            py,
            super::settings::Domain::from_core(cs705_core::icf::settings::Domain {
                min: 3,
                max: 9,
                step: 1,
            }),
        )
        .unwrap();
        setting.bind(py).setattr("domain", replacement).unwrap();
        assert_eq!(domain.getattr("min").unwrap().extract::<i64>().unwrap(), 3);
        let kind = py
            .get_type::<super::settings::Kind>()
            .getattr("Integer")
            .unwrap()
            .call1(("arbitrary owned unit",))
            .unwrap();
        setting.bind(py).setattr("kind", kind).unwrap();
        assert_eq!(
            setting
                .bind(py)
                .getattr("kind")
                .unwrap()
                .getattr("unit")
                .unwrap()
                .extract::<String>()
                .unwrap(),
            "arbitrary owned unit"
        );
    });
}

#[test]
fn borrowed_icf_python_wrapper_is_live_then_expires() {
    Python::initialize();
    Python::attach(|py| {
        let mut memory = vec![0; 0x3c9c0];
        memory[0x355a1] = 6;
        memory[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        let source = cs705_core::icf::Icf::from_memory(memory)
            .unwrap()
            .to_bytes();
        let mut native = cs705_core::icf::Icf::parse(&source).unwrap();
        let retained = crate::scoped::Frame::with_icf_mut(py, &mut native, |loan| {
            let view = Py::new(py, Icf::from_loan(loan))?;
            view.bind(py).call_method1("set_scope_mode_raw", (2,))?;
            assert_eq!(
                view.bind(py)
                    .call_method0("scope_mode_raw")?
                    .extract::<i32>()?,
                2
            );
            Ok(view)
        })
        .unwrap();
        assert_eq!(native.scope_mode_raw(), 2);
        let error = retained
            .bind(py)
            .call_method0("scope_mode_raw")
            .unwrap_err();
        assert!(error.is_instance_of::<PyReferenceError>(py));
        let error = retained.bind(py).call_method0("into_owned").unwrap_err();
        assert!(error.is_instance_of::<PyReferenceError>(py));
    });
}

#[test]
fn fixed_text_input_keeps_the_real_buffer_and_restores_failure_prefix() {
    use super::settings::extended::FixedTextInput;
    use pyo3::exceptions::{PyRuntimeError, PyValueError};
    Python::initialize();
    Python::attach(|py| {
        let source = Py::new(
            py,
            crate::conversion::MutableBytes::from_core(b"ab".to_vec()),
        )
        .unwrap();
        let input = py
            .get_type::<FixedTextInput>()
            .getattr("Text")
            .unwrap()
            .call1((source.clone_ref(py), 2))
            .unwrap()
            .extract::<Py<FixedTextInput>>()
            .unwrap();
        assert!(
            input
                .bind(py)
                .getattr("source")
                .unwrap()
                .is(source.bind(py))
        );
        let error = input
            .get()
            .with_core(py, |input| {
                let cs705_core::icf::settings::extended::FixedTextInput::Text {
                    source: bytes,
                    phase,
                } = input
                else {
                    panic!("wrong input variant")
                };
                assert_eq!(phase, 2);
                bytes[0] = b'z';
                let error = source.bind(py).call_method0("to_bytes").unwrap_err();
                assert!(error.is_instance_of::<PyRuntimeError>(py));
                Err::<(), _>(PyValueError::new_err("stop after source mutation"))
            })
            .unwrap_err();
        assert!(error.is_instance_of::<PyValueError>(py));
        source
            .get()
            .with_ref(|bytes| {
                assert_eq!(bytes, b"zb");
                Ok(())
            })
            .unwrap();
    });
}

#[test]
fn reset_state_ordinary_take_detaches_and_keeps_failure_prefix() {
    use pyo3::exceptions::PyRuntimeError;
    Python::initialize();
    Python::attach(|py| {
        let state = Py::new(
            py,
            super::normalizer::ResetState::from_core(cs705_core::icf::normalizer::ResetState {
                show_both: 0,
                home_invalidated: 0,
            }),
        )
        .unwrap();
        let mut native = state.get().take().unwrap();
        assert!(
            state
                .bind(py)
                .getattr("show_both")
                .unwrap_err()
                .is_instance_of::<PyRuntimeError>(py)
        );
        let result = py.detach(move || {
            native.show_both = 7;
            native.home_invalidated = 1;
            Err::<(), _>("stop after reset state mutation")
        });
        assert!(result.is_err());
        assert_eq!(
            state
                .bind(py)
                .getattr("show_both")
                .unwrap()
                .extract::<u32>()
                .unwrap(),
            7
        );
        assert_eq!(
            state
                .bind(py)
                .getattr("home_invalidated")
                .unwrap()
                .extract::<u8>()
                .unwrap(),
            1
        );
    });
}
