from pathlib import Path

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns

BASE_DIR = Path(__file__).resolve().parents[1]
OUTPUTS_DIR = BASE_DIR / "outputs"
DATA_DIR = BASE_DIR / "data"


def main() -> None:
    source = OUTPUTS_DIR / "analysis.csv"
    if not source.exists():
        source = DATA_DIR / "analysis.csv"
    if not source.exists():
        raise FileNotFoundError(
            "Add a CSV file at outputs/analysis.csv or data/analysis.csv before plotting."
        )

    OUTPUTS_DIR.mkdir(parents=True, exist_ok=True)

    df = pd.read_csv(source)
    numeric_columns = df.select_dtypes(include="number").columns.tolist()
    if len(numeric_columns) < 2:
        raise ValueError("Need at least two numeric columns to build the template plot.")

    x_col, y_col = numeric_columns[:2]

    sns.set_theme(style="whitegrid")
    fig, ax = plt.subplots(figsize=(8, 5))
    sns.lineplot(data=df, x=x_col, y=y_col, marker="o", ax=ax)
    ax.set_title("Analysis Plot")
    ax.set_xlabel(x_col)
    ax.set_ylabel(y_col)

    fig.tight_layout()
    fig.savefig(OUTPUTS_DIR / "plot.png", dpi=200)


if __name__ == "__main__":
    main()
