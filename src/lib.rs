use crate::binding::error::ValidationError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule, PyTuple};

pub mod binding;
pub mod codec;

fn init_core_types(m: &Bound<'_, PyModule>) -> PyResult<()> {
    binding::metaclass::add_struct_meta(m)?;
    m.add_class::<binding::core::Schema>()?;
    m.add_class::<binding::core::Struct>()?;
    m.add_class::<binding::core::StructConfig>()?;
    m.add_class::<binding::core::Meta>()?;
    m.add_class::<binding::core::TarsDict>()?;
    m.add("ValidationError", m.py().get_type::<ValidationError>())?;
    Ok(())
}

fn init_core_functions(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(binding::codec::ser::encode, m)?)?;
    m.add_function(wrap_pyfunction!(binding::codec::de::decode, m)?)?;
    m.add_function(wrap_pyfunction!(binding::codec::raw::encode_raw, m)?)?;
    m.add_function(wrap_pyfunction!(binding::codec::raw::decode_raw, m)?)?;
    m.add_function(wrap_pyfunction!(binding::codec::raw::probe_struct, m)?)?;
    m.add_class::<binding::codec::trace::TraceNode>()?;
    m.add_function(wrap_pyfunction!(binding::codec::trace::decode_trace, m)?)?;
    Ok(())
}

fn init_struct_class(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let struct_meta = m.getattr("StructMeta")?;
    let struct_base = m.getattr("_StructBase")?;
    let bases = PyTuple::new(py, vec![struct_base])?;
    let ns = PyDict::new(py);
    ns.set_item("__module__", "tarsio._core")?;
    let struct_cls = struct_meta.call1(("Struct", bases, ns))?;
    m.add("Struct", struct_cls)?;
    Ok(())
}

fn init_inspect_submodule(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let inspect_mod = PyModule::new(py, "tarsio._core.inspect")?;
    inspect_mod.add_class::<binding::inspect::TypeBase>()?;
    inspect_mod.add_class::<binding::inspect::BasicTypeBase>()?;
    inspect_mod.add_class::<binding::inspect::CompoundTypeBase>()?;
    inspect_mod.add_class::<binding::inspect::IntType>()?;
    inspect_mod.add_class::<binding::inspect::StrType>()?;
    inspect_mod.add_class::<binding::inspect::FloatType>()?;
    inspect_mod.add_class::<binding::inspect::BoolType>()?;
    inspect_mod.add_class::<binding::inspect::BytesType>()?;
    inspect_mod.add_class::<binding::inspect::AnyType>()?;
    inspect_mod.add_class::<binding::inspect::NoneType>()?;
    inspect_mod.add_class::<binding::inspect::EnumType>()?;
    inspect_mod.add_class::<binding::inspect::UnionType>()?;
    inspect_mod.add_class::<binding::inspect::ListType>()?;
    inspect_mod.add_class::<binding::inspect::TupleType>()?;
    inspect_mod.add_class::<binding::inspect::VarTupleType>()?;
    inspect_mod.add_class::<binding::inspect::MapType>()?;
    inspect_mod.add_class::<binding::inspect::SetType>()?;
    inspect_mod.add_class::<binding::inspect::OptionalType>()?;
    inspect_mod.add_class::<binding::inspect::StructType>()?;
    inspect_mod.add_class::<binding::inspect::RefType>()?;
    inspect_mod.add_class::<binding::inspect::Field>()?;
    inspect_mod.add("FieldInfo", inspect_mod.getattr("Field")?)?;
    inspect_mod.add_class::<binding::inspect::StructInfo>()?;
    inspect_mod.add_function(wrap_pyfunction!(binding::inspect::type_info, &inspect_mod)?)?;
    inspect_mod.add_function(wrap_pyfunction!(
        binding::inspect::struct_info,
        &inspect_mod
    )?)?;

    m.add("inspect", inspect_mod.as_any())?;

    let sys = py.import("sys")?;
    let modules_any = sys.getattr("modules")?;
    let modules = modules_any.cast::<PyDict>()?;
    modules.set_item("tarsio._core.inspect", inspect_mod.as_any())?;
    Ok(())
}

/// Rust 实现的 Python 模块.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    init_core_types(m)?;
    init_core_functions(m)?;
    init_struct_class(py, m)?;
    init_inspect_submodule(py, m)?;
    Ok(())
}
