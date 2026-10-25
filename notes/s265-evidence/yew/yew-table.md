| row (sagitta 9ace98c2 observer) | yew a: 280012fe on 0.2.34 | yew b: + compat on 0.2.35 | yew fix: + compat on the s265 head | b - a | fix - b |
|---|---|---|---|---:|---:|
| startup.first_paint.clean (--clean empty.c) | 2.506 / 2.485 / 2.529 ms | 2.521 / 2.569 / 2.558 | 2.527 / 2.546 / 2.561 | +43 us | -5 us |
| cold open to first paint (10,000-line fixture) | 24.669 / 24.663 / 24.536 ms | 24.740 / 24.765 / 24.637 | 24.524 / 24.752 / 24.609 | +91 us | -86 us |
