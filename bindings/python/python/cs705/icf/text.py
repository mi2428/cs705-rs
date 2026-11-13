"""Original-source reader, distinct from strict ICF parsing."""
from .._native import icf as _icf

Status = _icf.text.Status
Reader = _icf.text.Reader
Report = _icf.text.Report
coverage_status = _icf.text.coverage_status
extra_data_valid = _icf.text.extra_data_valid
decimal_prefix = _icf.text.decimal_prefix
hexadecimal_prefix = _icf.text.hexadecimal_prefix
fragments = _icf.text.fragments

__all__ = ["Status", "Reader", "Report", "coverage_status", "extra_data_valid", "decimal_prefix", "hexadecimal_prefix", "fragments"]
