"""
ClawProof ONNX Converter Sidecar

A FastAPI microservice that converts ML models from various frameworks
(PyTorch, TensorFlow/Keras, scikit-learn) into ONNX format.

The main ClawProof Rust server proxies multipart uploads to this service
when CONVERTER_URL is configured. See src/handlers/convert.rs.

Usage:
    uvicorn main:app --host 0.0.0.0 --port 8001
"""

from __future__ import annotations

import io
import logging
import os
import tempfile
import traceback
from pathlib import Path

from fastapi import FastAPI, File, Form, HTTPException, UploadFile
from fastapi.responses import Response

# ---------------------------------------------------------------------------
# Logging
# ---------------------------------------------------------------------------
logging.basicConfig(
    level=os.environ.get("LOG_LEVEL", "INFO").upper(),
    format="[converter] %(asctime)s %(levelname)s %(message)s",
)
logger = logging.getLogger("converter")

# ---------------------------------------------------------------------------
# Import guards for optional heavy dependencies
# ---------------------------------------------------------------------------
_TORCH_AVAILABLE = False
try:
    import torch
    import torch.onnx

    _TORCH_AVAILABLE = True
    logger.info("PyTorch %s available", torch.__version__)
except ImportError:
    logger.warning("PyTorch not installed -- .pt/.pth conversion disabled")

_TF_AVAILABLE = False
try:
    import tf2onnx

    _TF_AVAILABLE = True
    logger.info("tf2onnx %s available", tf2onnx.__version__)
except ImportError:
    logger.warning("tf2onnx not installed -- .pb (TensorFlow) conversion disabled")

_SKLEARN_AVAILABLE = False
try:
    import numpy as np
    from skl2onnx import convert_sklearn
    from skl2onnx.common.data_types import FloatTensorType

    _SKLEARN_AVAILABLE = True
    logger.info("skl2onnx available")
except ImportError:
    logger.warning("skl2onnx not installed -- .pkl (sklearn) conversion disabled")

try:
    import onnx  # noqa: F401 -- used for validation
except ImportError:
    logger.warning("onnx package not installed -- output validation disabled")

# ---------------------------------------------------------------------------
# FastAPI application
# ---------------------------------------------------------------------------
app = FastAPI(
    title="ClawProof ONNX Converter",
    description="Converts PyTorch, TensorFlow, and scikit-learn models to ONNX.",
    version="0.1.0",
)


# ---------------------------------------------------------------------------
# Health check
# ---------------------------------------------------------------------------
@app.get("/health")
async def health():
    return {
        "status": "ok",
        "backends": {
            "pytorch": _TORCH_AVAILABLE,
            "tensorflow": _TF_AVAILABLE,
            "sklearn": _SKLEARN_AVAILABLE,
        },
    }


# ---------------------------------------------------------------------------
# Conversion helpers
# ---------------------------------------------------------------------------

def _convert_pytorch(data: bytes, opset: int) -> bytes:
    """Convert a PyTorch .pt/.pth file to ONNX bytes."""
    if not _TORCH_AVAILABLE:
        raise HTTPException(
            status_code=501,
            detail="PyTorch is not installed in this converter instance.",
        )

    with tempfile.TemporaryDirectory() as tmpdir:
        src_path = Path(tmpdir) / "model.pt"
        src_path.write_bytes(data)

        # Attempt to load as a full model first, fall back to state_dict
        try:
            model = torch.load(str(src_path), map_location="cpu", weights_only=False)
        except Exception as exc:
            raise HTTPException(
                status_code=422,
                detail=(
                    f"Failed to load PyTorch model: {exc}. "
                    "Ensure the file is a complete model saved with "
                    "torch.save(model, path), not just a state_dict."
                ),
            )

        if not isinstance(model, torch.nn.Module):
            raise HTTPException(
                status_code=422,
                detail=(
                    "The uploaded file does not contain a torch.nn.Module. "
                    "Received type: {}. Save the full model with "
                    "torch.save(model, path).".format(type(model).__name__)
                ),
            )

        model.eval()

        # Infer a dummy input shape from the first parameter
        first_param = next(model.parameters(), None)
        if first_param is None:
            raise HTTPException(
                status_code=422,
                detail="Model has no parameters -- cannot infer input shape.",
            )
        in_features = first_param.shape[-1]
        dummy_input = torch.randn(1, in_features)

        onnx_path = Path(tmpdir) / "model.onnx"
        torch.onnx.export(
            model,
            dummy_input,
            str(onnx_path),
            opset_version=opset,
            input_names=["input"],
            output_names=["output"],
            dynamic_axes={"input": {0: "batch"}, "output": {0: "batch"}},
        )
        return onnx_path.read_bytes()


def _convert_tensorflow(data: bytes, opset: int) -> bytes:
    """Convert a TensorFlow SavedModel (.pb inside a directory) to ONNX bytes."""
    if not _TF_AVAILABLE:
        raise HTTPException(
            status_code=501,
            detail="tf2onnx is not installed in this converter instance.",
        )

    import subprocess
    import shutil

    with tempfile.TemporaryDirectory() as tmpdir:
        saved_model_dir = Path(tmpdir) / "saved_model"
        saved_model_dir.mkdir()

        # If the upload is a single .pb file, write it into the saved_model dir
        pb_path = saved_model_dir / "saved_model.pb"
        pb_path.write_bytes(data)

        onnx_path = Path(tmpdir) / "model.onnx"

        # Use tf2onnx CLI which handles TF session setup cleanly
        cmd = [
            "python", "-m", "tf2onnx.convert",
            "--saved-model", str(saved_model_dir),
            "--output", str(onnx_path),
            "--opset", str(opset),
        ]
        result = subprocess.run(
            cmd, capture_output=True, text=True, timeout=300,
        )

        if result.returncode != 0:
            detail = result.stderr.strip() or result.stdout.strip()
            # Truncate very long error output
            if len(detail) > 2000:
                detail = detail[:2000] + "... (truncated)"
            raise HTTPException(
                status_code=422,
                detail=f"tf2onnx conversion failed: {detail}",
            )

        if not onnx_path.exists():
            raise HTTPException(
                status_code=500,
                detail="tf2onnx did not produce an output file.",
            )
        return onnx_path.read_bytes()


def _convert_sklearn(data: bytes, opset: int) -> bytes:
    """Convert a scikit-learn .pkl model to ONNX bytes."""
    if not _SKLEARN_AVAILABLE:
        raise HTTPException(
            status_code=501,
            detail="skl2onnx is not installed in this converter instance.",
        )

    import pickle

    try:
        model = pickle.loads(data)
    except Exception as exc:
        raise HTTPException(
            status_code=422,
            detail=f"Failed to unpickle sklearn model: {exc}",
        )

    # Infer input dimension from the model
    n_features = None
    if hasattr(model, "n_features_in_"):
        n_features = int(model.n_features_in_)
    elif hasattr(model, "coef_"):
        coef = np.asarray(model.coef_)
        n_features = coef.shape[-1]
    elif hasattr(model, "feature_importances_"):
        n_features = len(model.feature_importances_)

    if n_features is None:
        raise HTTPException(
            status_code=422,
            detail=(
                "Cannot infer input feature count from the sklearn model. "
                "Ensure the model has been fitted before saving."
            ),
        )

    initial_type = [("input", FloatTensorType([None, n_features]))]

    try:
        onnx_model = convert_sklearn(
            model,
            initial_types=initial_type,
            target_opset=opset,
        )
    except Exception as exc:
        raise HTTPException(
            status_code=422,
            detail=f"skl2onnx conversion failed: {exc}",
        )

    return onnx_model.SerializeToString()


# Map of source_format values to converter functions
_CONVERTERS = {
    "pytorch": _convert_pytorch,
    "pt": _convert_pytorch,
    "pth": _convert_pytorch,
    "tensorflow": _convert_tensorflow,
    "tf": _convert_tensorflow,
    "pb": _convert_tensorflow,
    "sklearn": _convert_sklearn,
    "pkl": _convert_sklearn,
}


# ---------------------------------------------------------------------------
# POST /convert
# ---------------------------------------------------------------------------
@app.post(
    "/convert",
    summary="Convert a model to ONNX",
    response_class=Response,
    responses={
        200: {"content": {"application/octet-stream": {}}},
        400: {"description": "Missing or invalid parameters"},
        422: {"description": "Model conversion failed"},
        501: {"description": "Required backend not installed"},
    },
)
async def convert(
    file: UploadFile = File(..., description="The model file to convert"),
    source_format: str = Form(
        ...,
        description=(
            "Source framework: 'pytorch' (.pt/.pth), "
            "'tensorflow' (.pb), or 'sklearn' (.pkl)"
        ),
    ),
    opset: int = Form(
        13,
        description="ONNX opset version (default: 13)",
    ),
):
    """
    Accept an uploaded model file and a source_format identifier.
    Convert the model to ONNX and return the raw ONNX bytes.

    Supported source_format values:
      - pytorch / pt / pth   -- PyTorch model (.pt / .pth)
      - tensorflow / tf / pb -- TensorFlow SavedModel (.pb)
      - sklearn / pkl        -- scikit-learn model (.pkl)
    """
    source_format_lower = source_format.strip().lower()

    converter_fn = _CONVERTERS.get(source_format_lower)
    if converter_fn is None:
        raise HTTPException(
            status_code=400,
            detail=(
                f"Unsupported source_format: '{source_format}'. "
                f"Supported values: {sorted(set(_CONVERTERS.keys()))}"
            ),
        )

    logger.info(
        "Converting '%s' (format=%s, opset=%d)",
        file.filename,
        source_format_lower,
        opset,
    )

    # Read the uploaded file into memory
    try:
        data = await file.read()
    except Exception as exc:
        raise HTTPException(
            status_code=400,
            detail=f"Failed to read uploaded file: {exc}",
        )

    if len(data) == 0:
        raise HTTPException(
            status_code=400,
            detail="Uploaded file is empty.",
        )

    # Run the (potentially blocking) conversion in a thread so we don't
    # block the event loop.
    import asyncio

    loop = asyncio.get_running_loop()
    try:
        onnx_bytes = await loop.run_in_executor(
            None, converter_fn, data, opset
        )
    except HTTPException:
        raise
    except Exception as exc:
        logger.error("Conversion failed:\n%s", traceback.format_exc())
        raise HTTPException(
            status_code=500,
            detail=f"Unexpected conversion error: {exc}",
        )

    logger.info(
        "Conversion successful: %s -> %d bytes ONNX",
        file.filename,
        len(onnx_bytes),
    )

    return Response(
        content=onnx_bytes,
        media_type="application/octet-stream",
        headers={
            "Content-Disposition": "attachment; filename=model.onnx",
        },
    )
