"""Data types returned by the ClawProof API."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional


@dataclass
class FieldSchema:
    """Schema for a single structured input field accepted by a model."""

    name: str
    description: str
    min: int
    max: int


@dataclass
class Model:
    """Descriptor for a registered model on the ClawProof server."""

    id: str
    name: str
    description: str
    input_type: str  # "text" | "structured_fields" | "raw"
    input_dim: int
    input_shape: list[int]
    labels: list[str]
    trace_length: int
    fields: Optional[list[FieldSchema]] = None

    @classmethod
    def from_dict(cls, data: dict) -> Model:
        fields = None
        if data.get("fields") is not None:
            fields = [
                FieldSchema(
                    name=f["name"],
                    description=f["description"],
                    min=f["min"],
                    max=f["max"],
                )
                for f in data["fields"]
            ]
        return cls(
            id=data["id"],
            name=data["name"],
            description=data.get("description", ""),
            input_type=data["input_type"],
            input_dim=data["input_dim"],
            input_shape=data.get("input_shape", []),
            labels=data.get("labels", []),
            trace_length=data.get("trace_length", 0),
            fields=fields,
        )


@dataclass
class InferenceOutput:
    """The inference result returned alongside a proof receipt."""

    raw_output: list[int]
    predicted_class: int
    label: str
    confidence: float

    @classmethod
    def from_dict(cls, data: dict) -> InferenceOutput:
        return cls(
            raw_output=data.get("raw_output", []),
            predicted_class=data.get("predicted_class", 0),
            label=data.get("label", ""),
            confidence=data.get("confidence", 0.0),
        )


@dataclass
class Receipt:
    """A zkML proof receipt issued by the ClawProof server.

    Initially created with status ``"proving"``.  Once the proof is
    generated and verified the status transitions to ``"verified"``
    (or ``"failed"`` on error).
    """

    id: str
    model_id: str
    model_name: str
    status: str  # "proving" | "verified" | "failed"
    created_at: str
    output: InferenceOutput

    # Hashes
    model_hash: str = ""
    input_hash: str = ""
    output_hash: str = ""

    # Timestamps
    completed_at: Optional[str] = None

    # Proof metadata (populated after proving completes)
    proof_hash: Optional[str] = None
    proof_size: Optional[int] = None
    prove_time_ms: Optional[int] = None
    verify_time_ms: Optional[int] = None

    # Error info (populated on failure)
    error: Optional[str] = None

    # Extra fields returned by the /prove endpoint
    receipt_url: Optional[str] = None

    @classmethod
    def from_dict(cls, data: dict) -> Receipt:
        """Build a Receipt from a JSON dict.

        Handles both the ``/prove`` response shape (which includes
        ``receipt_id``) and the full ``/receipt/{id}`` response shape.
        """
        receipt_id = data.get("id") or data.get("receipt_id", "")
        output_data = data.get("output", {})
        output = InferenceOutput.from_dict(output_data) if output_data else InferenceOutput(
            raw_output=[], predicted_class=0, label="", confidence=0.0,
        )
        return cls(
            id=receipt_id,
            model_id=data.get("model_id", ""),
            model_name=data.get("model_name", ""),
            status=data.get("status", ""),
            created_at=data.get("created_at", ""),
            output=output,
            model_hash=data.get("model_hash", ""),
            input_hash=data.get("input_hash", ""),
            output_hash=data.get("output_hash", ""),
            completed_at=data.get("completed_at"),
            proof_hash=data.get("proof_hash"),
            proof_size=data.get("proof_size"),
            prove_time_ms=data.get("prove_time_ms"),
            verify_time_ms=data.get("verify_time_ms"),
            error=data.get("error"),
            receipt_url=data.get("receipt_url"),
        )
