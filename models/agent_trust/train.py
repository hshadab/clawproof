"""
Agent Trust Classifier — training and ONNX export.

Trains a small 2-layer MLP that classifies agent interactions as
TRUSTED / SUSPICIOUS / REJECT based on 7 structured fields from
Moltbook-style agent metadata.

Architecture matches ClawProof's existing models:
  input(44) → Gemm(44→24) → ReLU → Gemm(24→3) → output

Usage:
    python train.py          # trains + exports network.onnx, vocab.json, model.toml
    python train.py --test   # also runs a quick sanity check
"""

import json
import pathlib
import argparse

import numpy as np
import torch
import torch.nn as nn
import torch.optim as optim

# ---------------------------------------------------------------------------
# Feature definitions — each (name, num_values)
# ---------------------------------------------------------------------------
FIELDS = [
    ("karma",              11),   # 0-10: bucketed Moltbook karma
    ("account_age",         8),   # 0-7:  days since registration (bucketed)
    ("follower_ratio",      6),   # 0-5:  followers/following ratio (bucketed)
    ("post_frequency",      6),   # 0-5:  posts per day (bucketed)
    ("verification",        3),   # 0=none, 1=email, 2=X-verified
    ("content_similarity",  6),   # 0-5:  similarity to known spam (bucketed)
    ("interaction_type",    4),   # 0=post, 1=comment, 2=DM, 3=trade
]

INPUT_DIM = sum(n for _, n in FIELDS)  # 44
NUM_CLASSES = 3
LABELS = ["TRUSTED", "SUSPICIOUS", "REJECT"]

# ---------------------------------------------------------------------------
# Labeling heuristic — encodes domain knowledge about agent trust
# ---------------------------------------------------------------------------
def label_sample(karma, account_age, follower_ratio, post_frequency,
                 verification, content_similarity, interaction_type):
    """
    Rule-based labeler that produces realistic training signal.

    Key signals:
      - High verification + high karma + low spam similarity → TRUSTED
      - New account + high spam + DM/trade interaction      → REJECT
      - Everything else                                      → SUSPICIOUS
    """
    trust_score = 0

    # Karma (strong positive signal)
    trust_score += karma * 2

    # Account age (established agents are more trustworthy)
    trust_score += account_age * 3

    # Verification is the strongest single signal
    trust_score += verification * 8

    # Follower ratio — organic agents have balanced ratios
    if follower_ratio >= 2 and follower_ratio <= 4:
        trust_score += 4
    elif follower_ratio == 0:
        trust_score -= 3

    # Post frequency — moderate is good, extreme is suspicious
    if post_frequency >= 1 and post_frequency <= 3:
        trust_score += 2
    elif post_frequency >= 5:
        trust_score -= 5  # spammy

    # Content similarity to known spam (strong negative signal)
    trust_score -= content_similarity * 4

    # Interaction type risk
    if interaction_type == 3:  # trade
        trust_score -= 3  # higher-stakes interaction needs more trust
    elif interaction_type == 2:  # DM
        trust_score -= 2

    # Add noise for realistic boundary cases
    trust_score += np.random.normal(0, 3)

    if trust_score >= 20:
        return 0  # TRUSTED
    elif trust_score <= 5:
        return 2  # REJECT
    else:
        return 1  # SUSPICIOUS


# ---------------------------------------------------------------------------
# Data generation
# ---------------------------------------------------------------------------
def generate_dataset(n_samples=20000):
    """Generate synthetic training data with one-hot encoding."""
    X = np.zeros((n_samples, INPUT_DIM), dtype=np.float32)
    y = np.zeros(n_samples, dtype=np.int64)

    for i in range(n_samples):
        # Sample raw field values
        karma = np.random.randint(0, 11)
        account_age = np.random.randint(0, 8)
        follower_ratio = np.random.randint(0, 6)
        post_frequency = np.random.randint(0, 6)
        verification = np.random.choice([0, 0, 0, 1, 1, 2])  # skew toward unverified
        content_similarity = np.random.choice([0, 0, 0, 1, 1, 2, 3, 4, 5])  # skew low
        interaction_type = np.random.choice([0, 0, 1, 1, 1, 2, 3])  # mostly posts/comments

        # One-hot encode
        offset = 0
        for field_name, n_vals in FIELDS:
            val = locals()[field_name]
            X[i, offset + val] = 1.0
            offset += n_vals

        y[i] = label_sample(karma, account_age, follower_ratio, post_frequency,
                            verification, content_similarity, interaction_type)

    return X, y


# ---------------------------------------------------------------------------
# Model
# ---------------------------------------------------------------------------
class AgentTrustClassifier(nn.Module):
    def __init__(self, input_dim=INPUT_DIM, hidden_dim=24, num_classes=NUM_CLASSES):
        super().__init__()
        self.fc1 = nn.Linear(input_dim, hidden_dim)
        self.relu1 = nn.ReLU()
        self.fc2 = nn.Linear(hidden_dim, num_classes)

    def forward(self, x):
        x = self.fc1(x)
        x = self.relu1(x)
        x = self.fc2(x)
        return x


# ---------------------------------------------------------------------------
# Training
# ---------------------------------------------------------------------------
def train_model():
    np.random.seed(42)
    torch.manual_seed(42)

    X_train, y_train = generate_dataset(20000)
    X_val, y_val = generate_dataset(3000)

    X_train_t = torch.from_numpy(X_train)
    y_train_t = torch.from_numpy(y_train)
    X_val_t = torch.from_numpy(X_val)
    y_val_t = torch.from_numpy(y_val)

    model = AgentTrustClassifier()
    criterion = nn.CrossEntropyLoss()
    optimizer = optim.Adam(model.parameters(), lr=0.005)

    best_val_acc = 0.0
    best_state = None

    for epoch in range(150):
        model.train()

        # Mini-batch training
        perm = torch.randperm(len(X_train_t))
        total_loss = 0.0
        for start in range(0, len(X_train_t), 256):
            idx = perm[start:start + 256]
            batch_x, batch_y = X_train_t[idx], y_train_t[idx]

            optimizer.zero_grad()
            out = model(batch_x)
            loss = criterion(out, batch_y)
            loss.backward()
            optimizer.step()
            total_loss += loss.item()

        # Validation
        model.eval()
        with torch.no_grad():
            val_out = model(X_val_t)
            val_pred = val_out.argmax(dim=1)
            val_acc = (val_pred == y_val_t).float().mean().item()
            if val_acc > best_val_acc:
                best_val_acc = val_acc
                best_state = {k: v.clone() for k, v in model.state_dict().items()}

        if (epoch + 1) % 25 == 0:
            print(f"  epoch {epoch+1:3d}  loss={total_loss:.3f}  val_acc={val_acc:.4f}")

    print(f"\nBest validation accuracy: {best_val_acc:.4f}")
    model.load_state_dict(best_state)

    # Print class distribution
    with torch.no_grad():
        pred = model(X_val_t).argmax(dim=1).numpy()
        for i, label in enumerate(LABELS):
            count = (pred == i).sum()
            true_count = (y_val == i).sum()
            print(f"  {label:12s}  predicted={count:5d}  actual={true_count:5d}")

    return model


# ---------------------------------------------------------------------------
# Export ONNX
# ---------------------------------------------------------------------------
def export_onnx(model, out_dir):
    model.eval()
    dummy = torch.zeros(1, INPUT_DIM)
    path = out_dir / "network.onnx"

    # Build ONNX manually to avoid torch.onnx.export onnxscript compatibility
    # issues with newer PyTorch versions. The model is simple enough:
    #   input → Gemm(fc1) → ReLU → Gemm(fc2) → output
    import onnx
    from onnx import helper, TensorProto, numpy_helper

    state = model.state_dict()

    # Create initializers from model weights
    initializers = [
        numpy_helper.from_array(state["fc1.weight"].numpy(), name="fc1.weight"),
        numpy_helper.from_array(state["fc1.bias"].numpy(),   name="fc1.bias"),
        numpy_helper.from_array(state["fc2.weight"].numpy(), name="fc2.weight"),
        numpy_helper.from_array(state["fc2.bias"].numpy(),   name="fc2.bias"),
    ]

    # Build graph nodes: Gemm → ReLU → Gemm
    nodes = [
        helper.make_node("Gemm", ["input", "fc1.weight", "fc1.bias"],
                         ["/fc1/Gemm_output_0"],
                         alpha=1.0, beta=1.0, transB=1),
        helper.make_node("Relu", ["/fc1/Gemm_output_0"],
                         ["/relu1/Relu_output_0"]),
        helper.make_node("Gemm", ["/relu1/Relu_output_0", "fc2.weight", "fc2.bias"],
                         ["output"],
                         alpha=1.0, beta=1.0, transB=1),
    ]

    # I/O
    input_tensor = helper.make_tensor_value_info("input", TensorProto.FLOAT, ["batch_size", INPUT_DIM])
    output_tensor = helper.make_tensor_value_info("output", TensorProto.FLOAT, ["batch_size", NUM_CLASSES])

    graph = helper.make_graph(nodes, "main_graph", [input_tensor], [output_tensor], initializers)
    onnx_model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)])
    onnx_model.ir_version = 7
    onnx.checker.check_model(onnx_model)
    onnx.save(onnx_model, str(path))

    print(f"Exported ONNX model to {path}")
    return path


# ---------------------------------------------------------------------------
# Generate vocab.json
# ---------------------------------------------------------------------------
def generate_vocab(out_dir):
    vocab_mapping = {}
    feature_mapping = {}
    offset = 0

    for field_name, n_vals in FIELDS:
        keys = []
        for v in range(n_vals):
            key = f"{field_name}_{v}"
            vocab_mapping[key] = {
                "index": offset + v,
                "feature_type": field_name,
            }
            keys.append(key)
        feature_mapping[field_name] = keys
        offset += n_vals

    vocab = {
        "vocab_mapping": vocab_mapping,
        "feature_mapping": feature_mapping,
    }

    path = out_dir / "vocab.json"
    with open(path, "w") as f:
        json.dump(vocab, f, indent=2)
    print(f"Generated {path} ({offset} features)")
    return path


# ---------------------------------------------------------------------------
# Generate model.toml
# ---------------------------------------------------------------------------
def generate_toml(out_dir):
    field_defs = []
    for field_name, n_vals in FIELDS:
        descriptions = {
            "karma": "Moltbook karma score (bucketed 0-10)",
            "account_age": "Days since registration (bucketed 0-7)",
            "follower_ratio": "Followers/following ratio (bucketed 0-5)",
            "post_frequency": "Posts per day (bucketed 0-5)",
            "verification": "Verification level: 0=none, 1=email, 2=X-verified",
            "content_similarity": "Similarity to known spam content (bucketed 0-5)",
            "interaction_type": "Interaction: 0=post, 1=comment, 2=DM, 3=trade",
        }
        field_defs.append(
            f'[[fields]]\n'
            f'name = "{field_name}"\n'
            f'description = "{descriptions[field_name]}"\n'
            f'min = 0\n'
            f'max = {n_vals - 1}\n'
        )

    toml_content = (
        f'id = "agent_trust"\n'
        f'name = "Agent Trust Classifier"\n'
        f'description = "Classifies agent interactions as TRUSTED, SUSPICIOUS, or REJECT based on Moltbook-style agent metadata. Designed for verifiable agent-to-agent trust scoring."\n'
        f'input_type = "structured_fields"\n'
        f'input_dim = {INPUT_DIM}\n'
        f'input_shape = [1, {INPUT_DIM}]\n'
        f'labels = ["TRUSTED", "SUSPICIOUS", "REJECT"]\n'
        f'trace_length = 16384\n'
        f'\n'
        + '\n'.join(field_defs)
    )

    path = out_dir / "model.toml"
    with open(path, "w") as f:
        f.write(toml_content)
    print(f"Generated {path}")
    return path


# ---------------------------------------------------------------------------
# Sanity check
# ---------------------------------------------------------------------------
def sanity_check(onnx_path):
    """Quick test: load the ONNX model and run a few test cases."""
    import onnx
    from onnx import numpy_helper
    model = onnx.load(str(onnx_path))
    onnx.checker.check_model(model)
    print(f"ONNX model valid: {len(model.graph.node)} ops, "
          f"input={model.graph.input[0].type.tensor_type.shape}, "
          f"output={model.graph.output[0].type.tensor_type.shape}")

    # Quick inference with onnxruntime if available
    try:
        import onnxruntime as ort
        sess = ort.InferenceSession(str(onnx_path))

        test_cases = [
            # High-trust verified agent posting
            {"karma": 9, "account_age": 7, "follower_ratio": 3,
             "post_frequency": 2, "verification": 2, "content_similarity": 0,
             "interaction_type": 0},
            # New unverified agent DM-ing with spam-like content
            {"karma": 0, "account_age": 0, "follower_ratio": 0,
             "post_frequency": 5, "verification": 0, "content_similarity": 5,
             "interaction_type": 2},
            # Medium agent commenting
            {"karma": 4, "account_age": 3, "follower_ratio": 2,
             "post_frequency": 2, "verification": 1, "content_similarity": 1,
             "interaction_type": 1},
        ]

        for tc in test_cases:
            vec = np.zeros((1, INPUT_DIM), dtype=np.float32)
            offset = 0
            for field_name, n_vals in FIELDS:
                vec[0, offset + tc[field_name]] = 1.0
                offset += n_vals

            result = sess.run(None, {"input": vec})[0][0]
            pred = int(np.argmax(result))
            print(f"  {tc} → {LABELS[pred]} (logits: {result})")
    except ImportError:
        print("  (onnxruntime not available — skipping inference test)")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--test", action="store_true", help="Run sanity check after export")
    args = parser.parse_args()

    out_dir = pathlib.Path(__file__).parent
    print("=" * 60)
    print("Agent Trust Classifier — Training")
    print("=" * 60)
    print(f"Features: {len(FIELDS)} fields, {INPUT_DIM} one-hot dims")
    print(f"Classes:  {LABELS}")
    print()

    model = train_model()
    print()
    onnx_path = export_onnx(model, out_dir)
    generate_vocab(out_dir)
    generate_toml(out_dir)
    print()

    if args.test:
        print("Running sanity check...")
        sanity_check(onnx_path)

    print()
    print("Done! Model ready for ClawProof.")
    print(f"  curl -X POST <BASE>/prove \\")
    print(f'    -H "Content-Type: application/json" \\')
    print(f"    -d '{{\"model_id\":\"agent_trust\",\"input\":{{\"fields\":{{\"karma\":8,\"account_age\":5,\"follower_ratio\":3,\"post_frequency\":2,\"verification\":2,\"content_similarity\":0,\"interaction_type\":1}}}}}}'")


if __name__ == "__main__":
    main()
