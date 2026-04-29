# Black-box bot — Per-market emir-bazlı detay

Her marketin tüm emirleri tek tek incelendi: trigger, role, sinyal değerleri,
ve insan-okur yorum. Sparkline'lar (▁..█) tick verisini özetler.

**Veri:** `exports/blackbox-trades-20260429.csv` (307 emir, 6 market).

## btc-updown-5m-1777467000 — LOSE / unresolved

- 21 emir, son emir t_off=182s
- UP=14, DOWN=7
- Opener: t=6s, Up, score=5.0225, bsi=+0.0447, ofi=+0.6859
- Trigger dağılımı: `price_drift`=14, `pyramid_signal`=4, `signal_open`=1, `unknown`=1, `parity_gap`=1

### Sinyal trendi (5dk pencere)

```
signal_score [0-10]   : ▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▅▅▄▄▄▄▄▄▄▄▄▅▅▄▄▄▅▅▅▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▄▅▅▅▅▅▅▄▄▅▅▄
up_best_bid  [0-1]    : ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▆▆▆▆▆▆▆▆▆▆▆▆▇▇▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▆▆▅▅▅▅▅▅▅▅▅▅▅▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▄▅▄▅▄▅▄▄▄▄▄▅▆▆▆▆▆▃▄▂▅▇▂▇▇▇
down_best_bid [0-1]   : ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▄▄▄▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▁▁▁▁▁▁▁▁▂▂▂▂▂▂▁▁▁▁▁▁▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▄▃▄▃▄▃▃▄▄▄▄▂▂▂▂▂▂▄▃▅▂▁▄▁▁▁
ofi          [-1,1]   : ▇▇▇▇▇▇▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▆▆▆▇▇▇▇▆▆▆▆▆▅▅▅▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▆▆▆▆▇▆▆▆▆▇▇▇▇▇▇▆▆▆▆▆▆▆▆▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▄▄▄▄▄▄▄▄▄▄▄▄▄▄▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▄▄▄
```

### Emir-bazlı detay

```
t_off | outcome | size     | price  | score  | dscore  | ofi     | up_bid | down_bid | avg_dom | imbalance | candidate_trigger  | conf
----- | ---- | -------- | ------ | ------ | ------- | ------- | ------ | -------- | ------- | --------- | ------------------ | ----
6     | Up   | 40.0000  | 0.5400 | 5.0225 | +0.0000 | +0.6859 | 0.530  | 0.460    | 0.0000  | +0.000    | signal_open        | high
16    | Up   | 18.7800  | 0.5400 | 6.7403 | +1.7178 | +0.9634 | 0.580  | 0.410    | 0.5400  | +1.000    | unknown            | low 
18    | Up   | 41.0000  | 0.5800 | 6.8836 | +0.1433 | +0.9757 | 0.640  | 0.350    | 0.5400  | +1.000    | pyramid_signal     | medium
22    | Up   | 42.0000  | 0.5982 | 7.5647 | +0.6812 | +0.9348 | 0.640  | 0.350    | 0.5564  | +1.000    | pyramid_signal     | medium
22    | Up   | 43.0000  | 0.6400 | 7.5647 | +0.0000 | +0.9348 | 0.640  | 0.350    | 0.5688  | +1.000    | pyramid_signal     | medium
26    | Up   | 42.0000  | 0.6200 | 7.6513 | +0.0866 | +0.9347 | 0.620  | 0.370    | 0.5854  | +1.000    | pyramid_signal     | medium
38    | Down | 42.0000  | 0.3900 | 7.5135 | -0.1379 | +0.9392 | 0.620  | 0.370    | 0.5918  | +1.000    | parity_gap         | medium
50    | Up   | 44.0000  | 0.6335 | 7.7599 | +0.2464 | +0.6170 | 0.660  | 0.330    | 0.5918  | +0.687    | price_drift        | medium
50    | Up   | 1.5800   | 0.6600 | 7.7599 | +0.0000 | +0.6170 | 0.660  | 0.330    | 0.5986  | +0.731    | price_drift        | medium
50    | Up   | 10.0000  | 0.6500 | 7.7599 | +0.0000 | +0.6170 | 0.660  | 0.330    | 0.5989  | +0.733    | price_drift        | medium
54    | Down | 44.0000  | 0.3325 | 7.7217 | -0.0382 | +0.6257 | 0.600  | 0.390    | 0.6007  | +0.741    | price_drift        | medium
54    | Down | 42.0000  | 0.3526 | 7.7217 | +0.0000 | +0.6257 | 0.600  | 0.390    | 0.6007  | +0.533    | price_drift        | medium
58    | Down | 41.0000  | 0.4188 | 7.4595 | -0.2622 | +0.5495 | 0.570  | 0.420    | 0.6007  | +0.376    | price_drift        | medium
58    | Down | 5.0200   | 0.4300 | 7.4595 | +0.0000 | +0.5495 | 0.570  | 0.420    | 0.6007  | +0.251    | price_drift        | medium
68    | Up   | 42.0000  | 0.6100 | 7.7649 | +0.3054 | +0.6216 | 0.650  | 0.340    | 0.6007  | +0.237    | price_drift        | medium
74    | Down | 42.0000  | 0.3700 | 7.5102 | -0.2548 | +0.4509 | 0.660  | 0.330    | 0.6019  | +0.302    | price_drift        | medium
90    | Up   | 51.0000  | 0.7200 | 8.3067 | +0.7965 | +0.4796 | 0.740  | 0.250    | 0.6019  | +0.200    | price_drift        | medium
90    | Up   | 50.0000  | 0.6859 | 8.3067 | +0.0000 | +0.4796 | 0.740  | 0.250    | 0.6180  | +0.269    | price_drift        | medium
90    | Up   | 0.7573   | 0.7000 | 8.3067 | +0.0000 | +0.4796 | 0.740  | 0.250    | 0.6260  | +0.326    | price_drift        | medium
108   | Up   | 0.9000   | 0.8200 | 9.1247 | +0.8180 | +0.6557 | 0.840  | 0.150    | 0.6261  | +0.327    | price_drift        | medium
182   | Down | 14.9100  | 0.2400 | 9.2764 | +0.1517 | +0.7127 | 0.780  | 0.210    | 0.6265  | +0.328    | price_drift        | medium
```

### Emir yorumları

- **t=6s** (Up sz=40.0000 pr=0.5400, `signal_open`): İlk emir — score=5.02, intent=Up; bsi=+0.04, ofi=+0.69
- **t=16s** (Up sz=18.7800 pr=0.5400, `unknown`): Sınıflandırılamadı (review gerekli) — score=6.74, dscore=+1.72, intent_before=Up
- **t=18s** (Up sz=41.0000 pr=0.5800, `pyramid_signal`): Pyramid (dom=Up): ofi=0.98, score=6.88 (trend güçlü)
- **t=22s** (Up sz=42.0000 pr=0.5982, `pyramid_signal`): Pyramid (dom=Up): ofi=0.93, score=7.56 (trend güçlü)
- **t=22s** (Up sz=43.0000 pr=0.6400, `pyramid_signal`): Pyramid (dom=Up): ofi=0.93, score=7.56 (trend güçlü)
- **t=26s** (Up sz=42.0000 pr=0.6200, `pyramid_signal`): Pyramid (dom=Up): ofi=0.93, score=7.65 (trend güçlü)
- **t=38s** (Down sz=42.0000 pr=0.3900, `parity_gap`): Hedge top-up: |dom-opp|=227 share
- **t=50s** (Up sz=44.0000 pr=0.6335, `price_drift`): Fiyat hareketi → dom requote (price=0.6335)
- **t=50s** (Up sz=1.5800 pr=0.6600, `price_drift`): Fiyat hareketi → dom requote (price=0.6600)
- **t=50s** (Up sz=10.0000 pr=0.6500, `price_drift`): Fiyat hareketi → dom requote (price=0.6500)
- **t=54s** (Down sz=44.0000 pr=0.3325, `price_drift`): Fiyat hareketi → hedge requote (price=0.3325)
- **t=54s** (Down sz=42.0000 pr=0.3526, `price_drift`): Fiyat hareketi → hedge requote (price=0.3526)
- **t=58s** (Down sz=41.0000 pr=0.4188, `price_drift`): Fiyat hareketi → hedge requote (price=0.4188)
- **t=58s** (Down sz=5.0200 pr=0.4300, `price_drift`): Fiyat hareketi → hedge requote (price=0.4300)
- **t=68s** (Up sz=42.0000 pr=0.6100, `price_drift`): Fiyat hareketi → dom requote (price=0.6100)
- **t=74s** (Down sz=42.0000 pr=0.3700, `price_drift`): Fiyat hareketi → hedge requote (price=0.3700)
- **t=90s** (Up sz=51.0000 pr=0.7200, `price_drift`): Fiyat hareketi → dom requote (price=0.7200)
- **t=90s** (Up sz=50.0000 pr=0.6859, `price_drift`): Fiyat hareketi → dom requote (price=0.6859)
- **t=90s** (Up sz=0.7573 pr=0.7000, `price_drift`): Fiyat hareketi → dom requote (price=0.7000)
- **t=108s** (Up sz=0.9000 pr=0.8200, `price_drift`): Fiyat hareketi → dom requote (price=0.8200)
- **t=182s** (Down sz=14.9100 pr=0.2400, `price_drift`): Fiyat hareketi → hedge requote (price=0.2400)

---

## btc-updown-5m-1777467300 — WIN +1054.59 USDC

- 67 emir, son emir t_off=306s
- UP=40, DOWN=27
- Opener: t=10s, Down, score=4.3958, bsi=+5.8926, ofi=-0.0002
- Trigger dağılımı: `price_drift`=35, `parity_gap`=23, `unknown`=6, `signal_open`=1, `avg_down_edge`=1, `deadline_cleanup`=1

### Sinyal trendi (5dk pencere)

```
signal_score [0-10]   : ▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▄▄▄▄▃▃▃▃▃▄▄▃▃▄▄▄▄▃▃▃▃▃▃▃▃▃▃▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▃▃▃▃▃▃▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▃▃▃▃▃▃▃▃▃▃▃▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▄▄▄▄▃
up_best_bid  [0-1]    : ▁▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▄▄▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▄▄▄▄▄▄▄▄▄▄▄▄▃▃▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▂▂▂▂▂▂▂▂▂▂▃▃▃▃▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▃▃▃▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▁▁▁▂▁▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▂▃▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁
down_best_bid [0-1]   : ▁▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▄▄▄▄▄▄▄▄▄▄▄▄▅▅▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▆▅▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇
ofi          [-1,1]   : ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▆▆▆▇▄▅▅▅▅▅▅▅▅▅▅▅▅▅▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▇▇▆▇▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▂▂▂▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▂▂▂▃▃▃▃▃▃▃▃▃▃▃▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▁▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▆▆▅▅▄
```

### Emir-bazlı detay

```
t_off | outcome | size     | price  | score  | dscore  | ofi     | up_bid | down_bid | avg_dom | imbalance | candidate_trigger  | conf
----- | ---- | -------- | ------ | ------ | ------- | ------- | ------ | -------- | ------- | --------- | ------------------ | ----
10    | Down | 40.0000  | 0.5100 | 4.3958 | +0.0000 | -0.0002 | 0.440  | 0.550    | 0.0000  | +0.000    | signal_open        | high
20    | Down | 42.0000  | 0.5900 | 3.9227 | -0.4731 | +0.0017 | 0.390  | 0.600    | 0.5100  | +1.000    | price_drift        | medium
26    | Down | 45.0000  | 0.6600 | 3.5810 | -0.3417 | +0.0003 | 0.320  | 0.670    | 0.5510  | +1.000    | price_drift        | medium
26    | Down | 43.0000  | 0.6300 | 3.5810 | +0.0000 | +0.0003 | 0.320  | 0.670    | 0.5896  | +1.000    | price_drift        | medium
26    | Down | 44.0000  | 0.6431 | 3.5810 | +0.0000 | +0.0003 | 0.320  | 0.670    | 0.5998  | +1.000    | price_drift        | medium
32    | Up   | 43.0000  | 0.3255 | 3.5818 | +0.0008 | +0.0315 | 0.350  | 0.640    | 0.6087  | +1.000    | parity_gap         | medium
40    | Down | 47.0000  | 0.6700 | 3.6159 | +0.0341 | +0.0451 | 0.330  | 0.660    | 0.6087  | +0.665    | price_drift        | medium
50    | Up   | 43.0000  | 0.3669 | 3.8166 | +0.2007 | +0.0336 | 0.390  | 0.600    | 0.6198  | +0.717    | price_drift        | medium
50    | Up   | 42.0000  | 0.3782 | 3.8166 | +0.0000 | +0.0336 | 0.390  | 0.600    | 0.6198  | +0.504    | price_drift        | medium
50    | Up   | 42.0000  | 0.3764 | 3.8166 | +0.0000 | +0.0336 | 0.390  | 0.600    | 0.6198  | +0.342    | parity_gap         | medium
60    | Down | 43.0000  | 0.6200 | 4.1962 | +0.3796 | +0.1615 | 0.380  | 0.610    | 0.6198  | +0.211    | price_drift        | medium
60    | Down | 43.0000  | 0.6121 | 4.1962 | +0.0000 | +0.1615 | 0.380  | 0.610    | 0.6198  | +0.283    | unknown            | low 
66    | Up   | 43.0000  | 0.3500 | 4.2779 | +0.0817 | +0.1686 | 0.350  | 0.640    | 0.6188  | +0.342    | price_drift        | medium
78    | Down | 27.7900  | 0.6500 | 3.9768 | -0.3011 | +0.0995 | 0.450  | 0.540    | 0.6188  | +0.239    | price_drift        | medium
80    | Up   | 30.6600  | 0.4000 | 4.2850 | +0.3082 | +0.0449 | 0.450  | 0.540    | 0.6212  | +0.275    | price_drift        | medium
80    | Up   | 40.0000  | 0.4532 | 4.2850 | +0.0000 | +0.0449 | 0.450  | 0.540    | 0.6212  | +0.212    | price_drift        | medium
80    | Up   | 42.0000  | 0.3900 | 4.2850 | +0.0000 | +0.0449 | 0.450  | 0.540    | 0.6212  | +0.138    | price_drift        | medium
90    | Up   | 40.0000  | 0.4772 | 5.1056 | +0.8207 | +0.1970 | 0.410  | 0.580    | 0.6212  | +0.070    | price_drift        | medium
92    | Down | 41.0000  | 0.5500 | 5.0159 | -0.0898 | +0.2214 | 0.440  | 0.550    | 0.6212  | +0.012    | avg_down_edge      | high
92    | Down | 41.0000  | 0.5476 | 5.0159 | +0.0000 | +0.2214 | 0.440  | 0.550    | 0.6141  | +0.064    | unknown            | low 
92    | Down | 40.0000  | 0.5360 | 5.0159 | +0.0000 | +0.2214 | 0.440  | 0.550    | 0.6082  | +0.111    | price_drift        | medium
132   | Down | 40.0000  | 0.5400 | 6.0955 | +1.0796 | +0.5900 | 0.460  | 0.530    | 0.6024  | +0.152    | unknown            | low 
142   | Up   | 40.0000  | 0.4800 | 6.1434 | +0.0478 | +0.6500 | 0.480  | 0.510    | 0.5977  | +0.190    | parity_gap         | medium
142   | Up   | 40.0000  | 0.4745 | 6.1434 | +0.0000 | +0.6500 | 0.480  | 0.510    | 0.5977  | +0.139    | parity_gap         | medium
160   | Up   | 41.0000  | 0.5600 | 6.3815 | +0.2381 | +0.4569 | 0.550  | 0.440    | 0.5977  | +0.093    | price_drift        | medium
160   | Up   | 40.0000  | 0.5000 | 6.3815 | +0.0000 | +0.4569 | 0.550  | 0.440    | 0.5977  | +0.049    | price_drift        | medium
164   | Down | 41.0000  | 0.5600 | 6.3223 | -0.0591 | +0.4674 | 0.420  | 0.570    | 0.5977  | +0.010    | price_drift        | medium
164   | Down | 30.9100  | 0.4591 | 6.3223 | +0.0000 | +0.4674 | 0.420  | 0.570    | 0.5950  | +0.046    | price_drift        | medium
166   | Down | 1.8868   | 0.4700 | 6.0147 | -0.3076 | +0.4129 | 0.430  | 0.560    | 0.5881  | +0.072    | price_drift        | medium
166   | Down | 41.0000  | 0.5665 | 6.0147 | +0.0000 | +0.4129 | 0.430  | 0.560    | 0.5878  | +0.074    | price_drift        | medium
168   | Up   | 40.0000  | 0.4300 | 5.9792 | -0.0355 | +0.4133 | 0.450  | 0.540    | 0.5864  | +0.106    | price_drift        | medium
170   | Up   | 40.0000  | 0.4300 | 5.9914 | +0.0122 | +0.4191 | 0.450  | 0.540    | 0.5864  | +0.070    | parity_gap         | medium
170   | Up   | 40.0000  | 0.4361 | 5.9914 | +0.0000 | +0.4191 | 0.450  | 0.540    | 0.5864  | +0.036    | parity_gap         | medium
196   | Down | 60.0000  | 0.7400 | 2.1685 | -3.8229 | -0.8916 | 0.250  | 0.740    | 0.5864  | +0.004    | price_drift        | medium
202   | Up   | 45.0000  | 0.2742 | 2.3649 | +0.1963 | -0.8738 | 0.290  | 0.700    | 0.5994  | +0.048    | price_drift        | medium
202   | Up   | 49.0000  | 0.2600 | 2.3649 | +0.0000 | -0.8738 | 0.290  | 0.700    | 0.5994  | +0.014    | price_drift        | medium
222   | Up   | 36.6800  | 0.3127 | 2.5369 | +0.1720 | -0.7240 | 0.280  | 0.710    | 0.5994  | -0.020    | price_drift        | medium
222   | Up   | 42.0000  | 0.3056 | 2.5369 | +0.0000 | -0.7240 | 0.280  | 0.710    | 0.5994  | -0.044    | parity_gap         | medium
234   | Up   | 53.0000  | 0.2200 | 2.5125 | -0.0244 | -0.7500 | 0.260  | 0.730    | 0.5994  | -0.070    | price_drift        | medium
234   | Up   | 51.0000  | 0.2200 | 2.5125 | +0.0000 | -0.7500 | 0.260  | 0.730    | 0.5994  | -0.101    | parity_gap         | medium
236   | Up   | 47.0000  | 0.2109 | 2.5080 | -0.0045 | -0.7874 | 0.200  | 0.790    | 0.5994  | -0.130    | parity_gap         | medium
240   | Down | 7.0000   | 0.7900 | 2.3804 | -0.1276 | -0.7931 | 0.160  | 0.830    | 0.5994  | -0.154    | price_drift        | medium
240   | Down | 55.0000  | 0.7522 | 2.3804 | +0.0000 | -0.7931 | 0.160  | 0.830    | 0.6012  | -0.149    | price_drift        | medium
242   | Down | 8.2000   | 0.8400 | 2.3867 | +0.0064 | -0.7706 | 0.170  | 0.820    | 0.6120  | -0.113    | price_drift        | medium
242   | Down | 1.0500   | 0.8400 | 2.3867 | +0.0000 | -0.7706 | 0.170  | 0.820    | 0.6144  | -0.108    | unknown            | low 
242   | Down | 53.0000  | 0.7900 | 2.3867 | +0.0000 | -0.7706 | 0.170  | 0.820    | 0.6147  | -0.107    | price_drift        | medium
242   | Down | 6.2500   | 0.8400 | 2.3867 | +0.0000 | -0.7706 | 0.170  | 0.820    | 0.6258  | -0.074    | price_drift        | medium
242   | Down | 52.5000  | 0.8400 | 2.3867 | +0.0000 | -0.7706 | 0.170  | 0.820    | 0.6274  | -0.071    | unknown            | low 
242   | Down | 6.0000   | 0.8400 | 2.3867 | +0.0000 | -0.7706 | 0.170  | 0.820    | 0.6398  | -0.041    | unknown            | low 
262   | Down | 154.0000 | 0.9138 | 2.9630 | +0.5763 | -0.2421 | 0.090  | 0.900    | 0.6412  | -0.037    | price_drift        | medium
280   | Up   | 50.0800  | 0.1000 | 2.2353 | -0.7277 | -0.5753 | 0.260  | 0.730    | 0.6810  | +0.042    | price_drift        | medium
282   | Up   | 30.0000  | 0.1100 | 2.3272 | +0.0919 | -0.5558 | 0.220  | 0.770    | 0.6810  | +0.016    | parity_gap         | medium
282   | Up   | 5.6180   | 0.1100 | 2.3272 | +0.0000 | -0.5558 | 0.220  | 0.770    | 0.6810  | +0.002    | parity_gap         | low 
282   | Up   | 5.2841   | 0.1200 | 2.3272 | +0.0000 | -0.5558 | 0.220  | 0.770    | 0.6810  | -0.001    | parity_gap         | low 
282   | Up   | 16.5200  | 0.1200 | 2.3272 | +0.0000 | -0.5558 | 0.220  | 0.770    | 0.6810  | -0.003    | parity_gap         | medium
282   | Up   | 68.1818  | 0.1200 | 2.3272 | +0.0000 | -0.5558 | 0.220  | 0.770    | 0.6810  | -0.011    | parity_gap         | medium
282   | Up   | 5.0000   | 0.1200 | 2.3272 | +0.0000 | -0.5558 | 0.220  | 0.770    | 0.6810  | -0.042    | parity_gap         | medium
282   | Up   | 5.0000   | 0.1100 | 2.3272 | +0.0000 | -0.5558 | 0.220  | 0.770    | 0.6810  | -0.044    | parity_gap         | medium
282   | Up   | 1.0500   | 0.1100 | 2.3272 | +0.0000 | -0.5558 | 0.220  | 0.770    | 0.6810  | -0.046    | parity_gap         | medium
282   | Up   | 17.1348  | 0.1100 | 2.3272 | +0.0000 | -0.5558 | 0.220  | 0.770    | 0.6810  | -0.046    | parity_gap         | medium
282   | Up   | 5.4045   | 0.1100 | 2.3272 | +0.0000 | -0.5558 | 0.220  | 0.770    | 0.6810  | -0.054    | parity_gap         | medium
282   | Up   | 6.0000   | 0.1100 | 2.3272 | +0.0000 | -0.5558 | 0.220  | 0.770    | 0.6810  | -0.056    | parity_gap         | medium
282   | Up   | 6.0000   | 0.1100 | 2.3272 | +0.0000 | -0.5558 | 0.220  | 0.770    | 0.6810  | -0.058    | parity_gap         | medium
282   | Up   | 22.2222  | 0.1000 | 2.3272 | +0.0000 | -0.5558 | 0.220  | 0.770    | 0.6810  | -0.061    | parity_gap         | medium
282   | Up   | 10.0000  | 0.1100 | 2.3272 | +0.0000 | -0.5558 | 0.220  | 0.770    | 0.6810  | -0.070    | parity_gap         | medium
284   | Up   | 102.0000 | 0.0700 | 2.8130 | +0.4858 | -0.5582 | 0.120  | 0.870    | 0.6810  | -0.074    | price_drift        | medium
306   | Up   | 4358.0000 | 0.0100 | 3.9977 | +1.1847 | +0.0229 | 0.000  | 0.990    | 0.6810  | -0.114    | deadline_cleanup   | high
```

### Emir yorumları

- **t=10s** (Down sz=40.0000 pr=0.5100, `signal_open`): İlk emir — score=4.40, intent=Down; bsi=+5.89, ofi=-0.00
- **t=20s** (Down sz=42.0000 pr=0.5900, `price_drift`): Fiyat hareketi → dom requote (price=0.5900)
- **t=26s** (Down sz=45.0000 pr=0.6600, `price_drift`): Fiyat hareketi → dom requote (price=0.6600)
- **t=26s** (Down sz=43.0000 pr=0.6300, `price_drift`): Fiyat hareketi → dom requote (price=0.6300)
- **t=26s** (Down sz=44.0000 pr=0.6431, `price_drift`): Fiyat hareketi → dom requote (price=0.6431)
- **t=32s** (Up sz=43.0000 pr=0.3255, `parity_gap`): Hedge top-up: |dom-opp|=214 share
- **t=40s** (Down sz=47.0000 pr=0.6700, `price_drift`): Fiyat hareketi → dom requote (price=0.6700)
- **t=50s** (Up sz=43.0000 pr=0.3669, `price_drift`): Fiyat hareketi → hedge requote (price=0.3669)
- **t=50s** (Up sz=42.0000 pr=0.3782, `price_drift`): Fiyat hareketi → hedge requote (price=0.3782)
- **t=50s** (Up sz=42.0000 pr=0.3764, `parity_gap`): Hedge top-up: |dom-opp|=133 share
- **t=60s** (Down sz=43.0000 pr=0.6200, `price_drift`): Fiyat hareketi → dom requote (price=0.6200)
- **t=60s** (Down sz=43.0000 pr=0.6121, `unknown`): Sınıflandırılamadı (review gerekli) — score=4.20, dscore=+0.00, intent_before=Down
- **t=66s** (Up sz=43.0000 pr=0.3500, `price_drift`): Fiyat hareketi → hedge requote (price=0.3500)
- **t=78s** (Down sz=27.7900 pr=0.6500, `price_drift`): Fiyat hareketi → dom requote (price=0.6500)
- **t=80s** (Up sz=30.6600 pr=0.4000, `price_drift`): Fiyat hareketi → hedge requote (price=0.4000)
- **t=80s** (Up sz=40.0000 pr=0.4532, `price_drift`): Fiyat hareketi → hedge requote (price=0.4532)
- **t=80s** (Up sz=42.0000 pr=0.3900, `price_drift`): Fiyat hareketi → hedge requote (price=0.3900)
- **t=90s** (Up sz=40.0000 pr=0.4772, `price_drift`): Fiyat hareketi → hedge requote (price=0.4772)
- **t=92s** (Down sz=41.0000 pr=0.5500, `avg_down_edge`): Avg-down: dom avg=0.621, fiyat=0.5500 (7.1 tick aşağı)
- **t=92s** (Down sz=41.0000 pr=0.5476, `unknown`): Sınıflandırılamadı (review gerekli) — score=5.02, dscore=+0.00, intent_before=Down
- **t=92s** (Down sz=40.0000 pr=0.5360, `price_drift`): Fiyat hareketi → dom requote (price=0.5360)
- **t=132s** (Down sz=40.0000 pr=0.5400, `unknown`): Sınıflandırılamadı (review gerekli) — score=6.10, dscore=+1.08, intent_before=Down
- **t=142s** (Up sz=40.0000 pr=0.4800, `parity_gap`): Hedge top-up: |dom-opp|=171 share
- **t=142s** (Up sz=40.0000 pr=0.4745, `parity_gap`): Hedge top-up: |dom-opp|=131 share
- **t=160s** (Up sz=41.0000 pr=0.5600, `price_drift`): Fiyat hareketi → hedge requote (price=0.5600)
- **t=160s** (Up sz=40.0000 pr=0.5000, `price_drift`): Fiyat hareketi → hedge requote (price=0.5000)
- **t=164s** (Down sz=41.0000 pr=0.5600, `price_drift`): Fiyat hareketi → dom requote (price=0.5600)
- **t=164s** (Down sz=30.9100 pr=0.4591, `price_drift`): Fiyat hareketi → dom requote (price=0.4591)
- **t=166s** (Down sz=1.8868 pr=0.4700, `price_drift`): Fiyat hareketi → dom requote (price=0.4700)
- **t=166s** (Down sz=41.0000 pr=0.5665, `price_drift`): Fiyat hareketi → dom requote (price=0.5665)
- **t=168s** (Up sz=40.0000 pr=0.4300, `price_drift`): Fiyat hareketi → hedge requote (price=0.4300)
- **t=170s** (Up sz=40.0000 pr=0.4300, `parity_gap`): Hedge top-up: |dom-opp|=85 share
- **t=170s** (Up sz=40.0000 pr=0.4361, `parity_gap`): Hedge top-up: |dom-opp|=45 share
- **t=196s** (Down sz=60.0000 pr=0.7400, `price_drift`): Fiyat hareketi → dom requote (price=0.7400)
- **t=202s** (Up sz=45.0000 pr=0.2742, `price_drift`): Fiyat hareketi → hedge requote (price=0.2742)
- **t=202s** (Up sz=49.0000 pr=0.2600, `price_drift`): Fiyat hareketi → hedge requote (price=0.2600)
- **t=222s** (Up sz=36.6800 pr=0.3127, `price_drift`): Fiyat hareketi → hedge requote (price=0.3127)
- **t=222s** (Up sz=42.0000 pr=0.3056, `parity_gap`): Hedge top-up: |dom-opp|=66 share
- **t=234s** (Up sz=53.0000 pr=0.2200, `price_drift`): Fiyat hareketi → hedge requote (price=0.2200)
- **t=234s** (Up sz=51.0000 pr=0.2200, `parity_gap`): Hedge top-up: |dom-opp|=161 share
- **t=236s** (Up sz=47.0000 pr=0.2109, `parity_gap`): Hedge top-up: |dom-opp|=212 share
- **t=240s** (Down sz=7.0000 pr=0.7900, `price_drift`): Fiyat hareketi → dom requote (price=0.7900)
- **t=240s** (Down sz=55.0000 pr=0.7522, `price_drift`): Fiyat hareketi → dom requote (price=0.7522)
- **t=242s** (Down sz=8.2000 pr=0.8400, `price_drift`): Fiyat hareketi → dom requote (price=0.8400)
- **t=242s** (Down sz=1.0500 pr=0.8400, `unknown`): Sınıflandırılamadı (review gerekli) — score=2.39, dscore=+0.00, intent_before=Down
- **t=242s** (Down sz=53.0000 pr=0.7900, `price_drift`): Fiyat hareketi → dom requote (price=0.7900)
- **t=242s** (Down sz=6.2500 pr=0.8400, `price_drift`): Fiyat hareketi → dom requote (price=0.8400)
- **t=242s** (Down sz=52.5000 pr=0.8400, `unknown`): Sınıflandırılamadı (review gerekli) — score=2.39, dscore=+0.00, intent_before=Down
- **t=242s** (Down sz=6.0000 pr=0.8400, `unknown`): Sınıflandırılamadı (review gerekli) — score=2.39, dscore=+0.00, intent_before=Down
- **t=262s** (Down sz=154.0000 pr=0.9138, `price_drift`): Fiyat hareketi → dom requote (price=0.9138)
- **t=280s** (Up sz=50.0800 pr=0.1000, `price_drift`): Fiyat hareketi → hedge requote (price=0.1000)
- **t=282s** (Up sz=30.0000 pr=0.1100, `parity_gap`): Hedge top-up: |dom-opp|=34 share
- **t=282s** (Up sz=5.6180 pr=0.1100, `parity_gap`): Hedge top-up: |dom-opp|=4 share
- **t=282s** (Up sz=5.2841 pr=0.1200, `parity_gap`): Hedge top-up: |dom-opp|=1 share
- **t=282s** (Up sz=16.5200 pr=0.1200, `parity_gap`): Hedge top-up: |dom-opp|=7 share
- **t=282s** (Up sz=68.1818 pr=0.1200, `parity_gap`): Hedge top-up: |dom-opp|=23 share
- **t=282s** (Up sz=5.0000 pr=0.1200, `parity_gap`): Hedge top-up: |dom-opp|=91 share
- **t=282s** (Up sz=5.0000 pr=0.1100, `parity_gap`): Hedge top-up: |dom-opp|=96 share
- **t=282s** (Up sz=1.0500 pr=0.1100, `parity_gap`): Hedge top-up: |dom-opp|=101 share
- **t=282s** (Up sz=17.1348 pr=0.1100, `parity_gap`): Hedge top-up: |dom-opp|=102 share
- **t=282s** (Up sz=5.4045 pr=0.1100, `parity_gap`): Hedge top-up: |dom-opp|=120 share
- **t=282s** (Up sz=6.0000 pr=0.1100, `parity_gap`): Hedge top-up: |dom-opp|=125 share
- **t=282s** (Up sz=6.0000 pr=0.1100, `parity_gap`): Hedge top-up: |dom-opp|=131 share
- **t=282s** (Up sz=22.2222 pr=0.1000, `parity_gap`): Hedge top-up: |dom-opp|=137 share
- **t=282s** (Up sz=10.0000 pr=0.1100, `parity_gap`): Hedge top-up: |dom-opp|=159 share
- **t=284s** (Up sz=102.0000 pr=0.0700, `price_drift`): Fiyat hareketi → hedge requote (price=0.0700)
- **t=306s** (Up sz=4358.0000 pr=0.0100, `deadline_cleanup`): Deadline cleanup (t=306s, son saniyeler)

---

## btc-updown-5m-1777467600 — WIN +795.54 USDC

- 44 emir, son emir t_off=278s
- UP=25, DOWN=19
- Opener: t=4s, Up, score=3.5005, bsi=-4.9731, ofi=-0.7165
- 1 signal_flip:
  - t=208s Up→Down (Δscore=-1.3490)
- Trigger dağılımı: `price_drift`=30, `unknown`=7, `avg_down_edge`=2, `parity_gap`=2, `signal_open`=1, `pyramid_signal`=1, `signal_flip`=1

### Sinyal trendi (5dk pencere)

```
signal_score [0-10]   : ▄▄▄▄▃▃▃▃▃▃▃▃▃▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▃▃▄▄▄▄▄▄▄▄▄▄▄▅▅▅▆▆▆▆▆▇▇▇▇▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▆▆▆▇▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▃▄▄▄▃▄▃▃▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▄▄▄▄▄▄▄▄
up_best_bid  [0-1]    : ▁▄▄▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▅▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▆▆▆▆▆▆▆▆▆▇▆▇▇▇▇▆▇▆▆▆▆▆▇▇▆▆▆▆▆▇▇▇▆▆▆▆▆▆▆▆▅▅▅▅▄▄▄▄▄▄▄▄▄▄▄▄▅▄▄▄▅▅▅▅▅▅▅▅▆▅▅▅▅▅▆▅▆▆▆▆▆▆▄▄▃▃▂▃▆▁▁
down_best_bid [0-1]   : ▁▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▂▂▂▂▂▂▂▂▂▃▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▃▃▃▃▂▂▂▂▂▂▁▁▂▂▂▂▂▂▂▂▂▁▁▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▂▂▂▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▂▂▁▁▁▂▂▂▂▁▁▁▁▂▂▂▂▂▂▂▂▃▃▃▃▄▄▄▄▄▄▄▄▄▄▃▃▃▄▄▄▃▃▃▃▃▃▃▃▂▂▃▃▃▃▂▃▂▂▂▂▂▂▄▄▄▄▄▂▂▇▃
ofi          [-1,1]   : ▄▄▄▄▁▁▂▂▂▂▂▂▂▂▂▂▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▃▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▄▄▄▄▄▄▄▄▄▄▄▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▅▅▆▆▆▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▆▆▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▃▃▃▃▃▂▃▃▃▃▃▂▃▃▃▃▃▃▃▃▃▃▃▂▂▂▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▂▃▃▃▃▃▃▄▄▃▃▃
```

### Emir-bazlı detay

```
t_off | outcome | size     | price  | score  | dscore  | ofi     | up_bid | down_bid | avg_dom | imbalance | candidate_trigger  | conf
----- | ---- | -------- | ------ | ------ | ------- | ------- | ------ | -------- | ------- | --------- | ------------------ | ----
4     | Up   | 42.0000  | 0.5500 | 3.5005 | +0.0000 | -0.7165 | 0.440  | 0.550    | 0.0000  | +0.000    | signal_open        | high
4     | Up   | 41.0000  | 0.5490 | 3.5005 | +0.0000 | -0.7165 | 0.440  | 0.550    | 0.5500  | +1.000    | unknown            | low 
4     | Up   | 40.0000  | 0.5267 | 3.5005 | +0.0000 | -0.7165 | 0.440  | 0.550    | 0.5495  | +1.000    | avg_down_edge      | high
4     | Down | 40.0000  | 0.4751 | 3.5005 | +0.0000 | -0.7165 | 0.440  | 0.550    | 0.5421  | +1.000    | parity_gap         | medium
6     | Down | 6.9400   | 0.5400 | 3.4135 | -0.0870 | -0.7004 | 0.440  | 0.550    | 0.5421  | +0.509    | price_drift        | medium
10    | Down | 40.0000  | 0.5300 | 3.8353 | +0.4219 | -0.5855 | 0.590  | 0.400    | 0.5421  | +0.448    | price_drift        | medium
10    | Up   | 40.0000  | 0.4750 | 3.8353 | +0.0000 | -0.5855 | 0.590  | 0.400    | 0.5421  | +0.172    | price_drift        | medium
12    | Up   | 4.3488   | 0.5700 | 4.1875 | +0.3521 | -0.5686 | 0.630  | 0.360    | 0.5256  | +0.304    | price_drift        | medium
12    | Up   | 40.0000  | 0.4840 | 4.1875 | +0.0000 | -0.5686 | 0.630  | 0.360    | 0.5268  | +0.316    | price_drift        | medium
32    | Up   | 49.0000  | 0.6938 | 5.4312 | +1.2438 | -0.3904 | 0.730  | 0.260    | 0.5185  | +0.409    | price_drift        | medium
32    | Up   | 50.0000  | 0.7000 | 5.4312 | +0.0000 | -0.3904 | 0.730  | 0.260    | 0.5520  | +0.493    | unknown            | low 
38    | Up   | 55.0000  | 0.7400 | 5.5127 | +0.0815 | -0.5418 | 0.640  | 0.350    | 0.5762  | +0.558    | price_drift        | medium
40    | Up   | 47.4200  | 0.7663 | 4.6925 | -0.8203 | -0.5965 | 0.550  | 0.440    | 0.6011  | +0.612    | price_drift        | medium
40    | Up   | 4.3478   | 0.7700 | 4.6925 | +0.0000 | -0.5965 | 0.550  | 0.440    | 0.6203  | +0.649    | unknown            | low 
40    | Down | 35.0000  | 0.2457 | 4.6925 | +0.0000 | -0.5965 | 0.550  | 0.440    | 0.6219  | +0.652    | price_drift        | medium
54    | Up   | 9.7100   | 0.6100 | 4.6080 | -0.0845 | -0.5784 | 0.640  | 0.350    | 0.6219  | +0.544    | price_drift        | medium
54    | Up   | 41.0000  | 0.5530 | 4.6080 | +0.0000 | -0.5784 | 0.640  | 0.350    | 0.6216  | +0.552    | price_drift        | medium
54    | Up   | 7.9487   | 0.6100 | 4.6080 | +0.0000 | -0.5784 | 0.640  | 0.350    | 0.6155  | +0.584    | price_drift        | medium
54    | Up   | 10.0000  | 0.5850 | 4.6080 | +0.0000 | -0.5784 | 0.640  | 0.350    | 0.6154  | +0.589    | price_drift        | medium
54    | Up   | 5.1316   | 0.6200 | 4.6080 | +0.0000 | -0.5784 | 0.640  | 0.350    | 0.6148  | +0.596    | price_drift        | medium
56    | Up   | 31.7200  | 0.6200 | 4.7330 | +0.1250 | -0.5785 | 0.630  | 0.360    | 0.6148  | +0.599    | unknown            | low 
56    | Up   | 5.1316   | 0.6200 | 4.7330 | +0.0000 | -0.5785 | 0.630  | 0.360    | 0.6152  | +0.619    | unknown            | low 
72    | Up   | 78.0000  | 0.8500 | 8.5727 | +3.8397 | +0.2444 | 0.840  | 0.150    | 0.6152  | +0.622    | price_drift        | medium
72    | Up   | 36.0000  | 0.7903 | 8.5727 | +0.0000 | +0.2444 | 0.840  | 0.150    | 0.6456  | +0.663    | price_drift        | medium
86    | Down | 74.0000  | 0.1600 | 8.1671 | -0.4055 | +0.1592 | 0.740  | 0.250    | 0.6538  | +0.679    | price_drift        | medium
86    | Down | 68.0000  | 0.1600 | 8.1671 | +0.0000 | +0.1592 | 0.740  | 0.250    | 0.6538  | +0.530    | parity_gap         | medium
88    | Down | 45.0000  | 0.2000 | 7.8619 | -0.3052 | +0.1388 | 0.740  | 0.250    | 0.6538  | +0.415    | price_drift        | medium
88    | Down | 2.0200   | 0.1700 | 7.8619 | +0.0000 | +0.1388 | 0.740  | 0.250    | 0.6538  | +0.347    | price_drift        | medium
88    | Down | 23.1400  | 0.1865 | 7.8619 | +0.0000 | +0.1388 | 0.740  | 0.250    | 0.6538  | +0.344    | price_drift        | medium
100   | Up   | 56.0000  | 0.7500 | 8.8504 | +0.9885 | +0.7809 | 0.750  | 0.240    | 0.6538  | +0.312    | price_drift        | medium
106   | Up   | 60.0000  | 0.7839 | 9.1523 | +0.3018 | +0.7960 | 0.810  | 0.180    | 0.6616  | +0.350    | pyramid_signal     | medium
208   | Down | 44.7800  | 0.0900 | 7.8033 | -1.3490 | +0.0674 | 0.890  | 0.100    | 0.6713  | +0.386    | signal_flip        | high
212   | Down | 83.0000  | 0.1336 | 7.1609 | -0.6425 | -0.0783 | 0.830  | 0.160    | 0.2454  | -0.331    | avg_down_edge      | high
214   | Down | 78.0000  | 0.1500 | 6.8184 | -0.3424 | -0.0671 | 0.840  | 0.150    | 0.2253  | -0.240    | price_drift        | medium
228   | Down | 68.0000  | 0.1509 | 5.6823 | -1.1362 | -0.3791 | 0.850  | 0.140    | 0.2144  | -0.165    | unknown            | low 
246   | Down | 26.0000  | 0.1658 | 5.1898 | -0.4924 | -0.4057 | 0.800  | 0.190    | 0.2073  | -0.107    | price_drift        | medium
254   | Down | 17.9900  | 0.2200 | 4.1364 | -1.0535 | -0.4325 | 0.550  | 0.440    | 0.2056  | -0.086    | price_drift        | medium
254   | Down | 51.0000  | 0.2500 | 4.1364 | +0.0000 | -0.4325 | 0.550  | 0.440    | 0.2060  | -0.072    | price_drift        | medium
254   | Down | 52.0000  | 0.2451 | 4.1364 | +0.0000 | -0.4325 | 0.550  | 0.440    | 0.2092  | -0.035    | unknown            | low 
254   | Down | 0.6700   | 0.2100 | 4.1364 | +0.0000 | -0.4325 | 0.550  | 0.440    | 0.2117  | +0.001    | price_drift        | medium
266   | Up   | 41.0000  | 0.5144 | 4.3850 | +0.2487 | -0.2405 | 0.580  | 0.410    | 0.2117  | +0.001    | price_drift        | medium
266   | Up   | 42.0000  | 0.5264 | 4.3850 | +0.0000 | -0.2405 | 0.580  | 0.410    | 0.2117  | -0.025    | price_drift        | medium
270   | Down | 40.0000  | 0.5080 | 4.2187 | -0.1663 | -0.2754 | 0.580  | 0.380    | 0.2117  | -0.051    | price_drift        | medium
278   | Up   | 42.0000  | 0.6200 | 5.0309 | +0.8122 | -0.0192 | 0.720  | 0.270    | 0.2266  | -0.025    | price_drift        | medium
```

### Emir yorumları

- **t=4s** (Up sz=42.0000 pr=0.5500, `signal_open`): İlk emir — score=3.50, intent=Up; bsi=-4.97, ofi=-0.72
- **t=4s** (Up sz=41.0000 pr=0.5490, `unknown`): Sınıflandırılamadı (review gerekli) — score=3.50, dscore=+0.00, intent_before=Up
- **t=4s** (Up sz=40.0000 pr=0.5267, `avg_down_edge`): Avg-down: dom avg=0.549, fiyat=0.5267 (2.3 tick aşağı)
- **t=4s** (Down sz=40.0000 pr=0.4751, `parity_gap`): Hedge top-up: |dom-opp|=123 share
- **t=6s** (Down sz=6.9400 pr=0.5400, `price_drift`): Fiyat hareketi → hedge requote (price=0.5400)
- **t=10s** (Down sz=40.0000 pr=0.5300, `price_drift`): Fiyat hareketi → hedge requote (price=0.5300)
- **t=10s** (Up sz=40.0000 pr=0.4750, `price_drift`): Fiyat hareketi → dom requote (price=0.4750)
- **t=12s** (Up sz=4.3488 pr=0.5700, `price_drift`): Fiyat hareketi → dom requote (price=0.5700)
- **t=12s** (Up sz=40.0000 pr=0.4840, `price_drift`): Fiyat hareketi → dom requote (price=0.4840)
- **t=32s** (Up sz=49.0000 pr=0.6938, `price_drift`): Fiyat hareketi → dom requote (price=0.6938)
- **t=32s** (Up sz=50.0000 pr=0.7000, `unknown`): Sınıflandırılamadı (review gerekli) — score=5.43, dscore=+0.00, intent_before=Up
- **t=38s** (Up sz=55.0000 pr=0.7400, `price_drift`): Fiyat hareketi → dom requote (price=0.7400)
- **t=40s** (Up sz=47.4200 pr=0.7663, `price_drift`): Fiyat hareketi → dom requote (price=0.7663)
- **t=40s** (Up sz=4.3478 pr=0.7700, `unknown`): Sınıflandırılamadı (review gerekli) — score=4.69, dscore=+0.00, intent_before=Up
- **t=40s** (Down sz=35.0000 pr=0.2457, `price_drift`): Fiyat hareketi → hedge requote (price=0.2457)
- **t=54s** (Up sz=9.7100 pr=0.6100, `price_drift`): Fiyat hareketi → dom requote (price=0.6100)
- **t=54s** (Up sz=41.0000 pr=0.5530, `price_drift`): Fiyat hareketi → dom requote (price=0.5530)
- **t=54s** (Up sz=7.9487 pr=0.6100, `price_drift`): Fiyat hareketi → dom requote (price=0.6100)
- **t=54s** (Up sz=10.0000 pr=0.5850, `price_drift`): Fiyat hareketi → dom requote (price=0.5850)
- **t=54s** (Up sz=5.1316 pr=0.6200, `price_drift`): Fiyat hareketi → dom requote (price=0.6200)
- **t=56s** (Up sz=31.7200 pr=0.6200, `unknown`): Sınıflandırılamadı (review gerekli) — score=4.73, dscore=+0.12, intent_before=Up
- **t=56s** (Up sz=5.1316 pr=0.6200, `unknown`): Sınıflandırılamadı (review gerekli) — score=4.73, dscore=+0.00, intent_before=Up
- **t=72s** (Up sz=78.0000 pr=0.8500, `price_drift`): Fiyat hareketi → dom requote (price=0.8500)
- **t=72s** (Up sz=36.0000 pr=0.7903, `price_drift`): Fiyat hareketi → dom requote (price=0.7903)
- **t=86s** (Down sz=74.0000 pr=0.1600, `price_drift`): Fiyat hareketi → hedge requote (price=0.1600)
- **t=86s** (Down sz=68.0000 pr=0.1600, `parity_gap`): Hedge top-up: |dom-opp|=442 share
- **t=88s** (Down sz=45.0000 pr=0.2000, `price_drift`): Fiyat hareketi → hedge requote (price=0.2000)
- **t=88s** (Down sz=2.0200 pr=0.1700, `price_drift`): Fiyat hareketi → hedge requote (price=0.1700)
- **t=88s** (Down sz=23.1400 pr=0.1865, `price_drift`): Fiyat hareketi → hedge requote (price=0.1865)
- **t=100s** (Up sz=56.0000 pr=0.7500, `price_drift`): Fiyat hareketi → dom requote (price=0.7500)
- **t=106s** (Up sz=60.0000 pr=0.7839, `pyramid_signal`): Pyramid (dom=Up): ofi=0.80, score=9.15 (trend güçlü)
- **t=208s** (Down sz=44.7800 pr=0.0900, `signal_flip`): Yön değişti: Up→Down (Δscore=-1.35); eski dom emirler iptal edilmiş olmalı
- **t=212s** (Down sz=83.0000 pr=0.1336, `avg_down_edge`): Avg-down: dom avg=0.245, fiyat=0.1336 (11.2 tick aşağı)
- **t=214s** (Down sz=78.0000 pr=0.1500, `price_drift`): Fiyat hareketi → dom requote (price=0.1500)
- **t=228s** (Down sz=68.0000 pr=0.1509, `unknown`): Sınıflandırılamadı (review gerekli) — score=5.68, dscore=-1.14, intent_before=Down
- **t=246s** (Down sz=26.0000 pr=0.1658, `price_drift`): Fiyat hareketi → dom requote (price=0.1658)
- **t=254s** (Down sz=17.9900 pr=0.2200, `price_drift`): Fiyat hareketi → dom requote (price=0.2200)
- **t=254s** (Down sz=51.0000 pr=0.2500, `price_drift`): Fiyat hareketi → dom requote (price=0.2500)
- **t=254s** (Down sz=52.0000 pr=0.2451, `unknown`): Sınıflandırılamadı (review gerekli) — score=4.14, dscore=+0.00, intent_before=Down
- **t=254s** (Down sz=0.6700 pr=0.2100, `price_drift`): Fiyat hareketi → dom requote (price=0.2100)
- **t=266s** (Up sz=41.0000 pr=0.5144, `price_drift`): Fiyat hareketi → hedge requote (price=0.5144)
- **t=266s** (Up sz=42.0000 pr=0.5264, `price_drift`): Fiyat hareketi → hedge requote (price=0.5264)
- **t=270s** (Down sz=40.0000 pr=0.5080, `price_drift`): Fiyat hareketi → dom requote (price=0.5080)
- **t=278s** (Up sz=42.0000 pr=0.6200, `price_drift`): Fiyat hareketi → hedge requote (price=0.6200)

---

## btc-updown-5m-1777467900 — LOSE / unresolved

- 55 emir, son emir t_off=276s
- UP=29, DOWN=26
- Opener: t=16s, Up, score=5.3023, bsi=+0.9925, ofi=+0.1247
- 4 signal_flip:
  - t=42s Up→Down (Δscore=-2.9437)
  - t=110s Down→Up (Δscore=+1.4303)
  - t=152s Up→Down (Δscore=-3.5305)
  - t=242s Down→Up (Δscore=+2.6468)
- Trigger dağılımı: `price_drift`=29, `parity_gap`=9, `unknown`=9, `signal_flip`=4, `avg_down_edge`=3, `signal_open`=1

### Sinyal trendi (5dk pencere)

```
signal_score [0-10]   : ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▂▂▂▂▂▂▂▂▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▂▂▂▃▃▃▂▂▄▄▄▄▄▄▄▄▄▄▄▄▄▂▂▂▂▂▂▂▂▂▂▂▂▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▃▃▃▃▃▃▃▃▃▃▂▂▂▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃
up_best_bid  [0-1]    : ▁▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▃▃▃▃▃▄▄▄▄▃▃▃▃▃▃▃▃▃▃▃▃▂▂▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▂▂▂▂▂▂▂▂▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▂▂▂▂▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁
down_best_bid [0-1]   : ▁▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇
ofi          [-1,1]   : ▃▃▃▃▃▃▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▆▆▆▆▆▆▆▆▆▆▆▆▆▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▆▆▆▅▅▆▅▅▅▅▅▅▅▅▅▅▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
```

### Emir-bazlı detay

```
t_off | outcome | size     | price  | score  | dscore  | ofi     | up_bid | down_bid | avg_dom | imbalance | candidate_trigger  | conf
----- | ---- | -------- | ------ | ------ | ------- | ------- | ------ | -------- | ------- | --------- | ------------------ | ----
16    | Up   | 40.0000  | 0.4900 | 5.3023 | +0.0000 | +0.1247 | 0.570  | 0.410    | 0.0000  | +0.000    | signal_open        | high
16    | Up   | 40.0000  | 0.4718 | 5.3023 | +0.0000 | +0.1247 | 0.570  | 0.410    | 0.4900  | +1.000    | avg_down_edge      | high
20    | Up   | 40.0000  | 0.4951 | 5.7073 | +0.4050 | +0.1264 | 0.550  | 0.440    | 0.4809  | +1.000    | price_drift        | medium
24    | Down | 25.6900  | 0.4639 | 5.3065 | -0.4008 | +0.1192 | 0.480  | 0.510    | 0.4856  | +1.000    | parity_gap         | medium
42    | Down | 9.3300   | 0.5800 | 2.3628 | -2.9437 | -0.7367 | 0.310  | 0.680    | 0.4856  | +0.647    | signal_flip        | high
42    | Down | 40.0000  | 0.4651 | 2.3628 | +0.0000 | -0.7367 | 0.310  | 0.680    | 0.4948  | -0.548    | avg_down_edge      | high
42    | Down | 6.6304   | 0.5400 | 2.3628 | +0.0000 | -0.7367 | 0.310  | 0.680    | 0.4790  | -0.231    | price_drift        | medium
42    | Down | 23.5600  | 0.5800 | 2.3628 | +0.0000 | -0.7367 | 0.310  | 0.680    | 0.4839  | -0.190    | price_drift        | medium
42    | Down | 2.3810   | 0.5800 | 2.3628 | +0.0000 | -0.7367 | 0.310  | 0.680    | 0.5054  | -0.066    | unknown            | low 
42    | Down | 5.7143   | 0.5800 | 2.3628 | +0.0000 | -0.7367 | 0.310  | 0.680    | 0.5071  | -0.055    | unknown            | low 
68    | Down | 65.0000  | 0.7978 | 1.0745 | -1.2883 | -0.8623 | 0.180  | 0.810    | 0.5108  | -0.029    | price_drift        | medium
70    | Down | 63.0000  | 0.7884 | 0.9248 | -0.1496 | -0.8622 | 0.170  | 0.820    | 0.6154  | +0.195    | unknown            | low 
70    | Down | 74.0000  | 0.8088 | 0.9248 | +0.0000 | -0.8622 | 0.170  | 0.820    | 0.6606  | +0.336    | price_drift        | medium
76    | Up   | 71.0000  | 0.1603 | 1.2933 | +0.3684 | -0.8679 | 0.240  | 0.750    | 0.6953  | +0.449    | price_drift        | medium
82    | Up   | 3.5256   | 0.2200 | 1.2347 | -0.0586 | -0.9252 | 0.250  | 0.740    | 0.6953  | +0.246    | price_drift        | medium
84    | Up   | 53.0000  | 0.2300 | 1.3330 | +0.0984 | -0.9249 | 0.200  | 0.790    | 0.6953  | +0.237    | parity_gap         | medium
84    | Up   | 52.0000  | 0.2300 | 1.3330 | +0.0000 | -0.9249 | 0.200  | 0.790    | 0.6953  | +0.120    | parity_gap         | medium
86    | Down | 17.0000  | 0.7600 | 1.2179 | -0.1151 | -0.9249 | 0.200  | 0.790    | 0.6953  | +0.026    | price_drift        | medium
86    | Down | 38.0000  | 0.7563 | 1.2179 | +0.0000 | -0.9249 | 0.200  | 0.790    | 0.6986  | +0.052    | unknown            | low 
86    | Down | 58.0000  | 0.7700 | 1.2179 | +0.0000 | -0.9249 | 0.200  | 0.790    | 0.7046  | +0.106    | price_drift        | medium
88    | Down | 60.0000  | 0.7700 | 1.1325 | -0.0854 | -0.9248 | 0.190  | 0.800    | 0.7134  | +0.177    | unknown            | low 
88    | Up   | 2.5500   | 0.2000 | 1.1325 | +0.0000 | -0.9248 | 0.190  | 0.800    | 0.7204  | +0.240    | price_drift        | medium
88    | Up   | 1.2500   | 0.2000 | 1.1325 | +0.0000 | -0.9248 | 0.190  | 0.800    | 0.7204  | +0.236    | parity_gap         | medium
88    | Up   | 2.5000   | 0.2000 | 1.1325 | +0.0000 | -0.9248 | 0.190  | 0.800    | 0.7204  | +0.234    | parity_gap         | medium
102   | Up   | 53.0000  | 0.2500 | 1.3474 | +0.2150 | -0.6085 | 0.320  | 0.660    | 0.7204  | +0.230    | price_drift        | medium
102   | Up   | 56.0000  | 0.2248 | 1.3474 | +0.0000 | -0.6085 | 0.320  | 0.660    | 0.7204  | +0.153    | price_drift        | medium
102   | Up   | 55.0000  | 0.2377 | 1.3474 | +0.0000 | -0.6085 | 0.320  | 0.660    | 0.7204  | +0.081    | price_drift        | medium
110   | Up   | 41.0000  | 0.4200 | 2.7777 | +1.4303 | -0.5322 | 0.430  | 0.560    | 0.7204  | +0.019    | signal_flip        | high
110   | Up   | 29.8900  | 0.4000 | 2.7777 | +0.0000 | -0.5322 | 0.430  | 0.560    | 0.2975  | +0.023    | price_drift        | medium
110   | Up   | 20.3900  | 0.3900 | 2.7777 | +0.0000 | -0.5322 | 0.430  | 0.560    | 0.3032  | +0.051    | price_drift        | medium
110   | Up   | 12.1100  | 0.4000 | 2.7777 | +0.0000 | -0.5322 | 0.430  | 0.560    | 0.3063  | +0.069    | price_drift        | medium
110   | Up   | 19.5200  | 0.3800 | 2.7777 | +0.0000 | -0.5322 | 0.430  | 0.560    | 0.3083  | +0.080    | price_drift        | medium
110   | Up   | 11.4300  | 0.3700 | 2.7777 | +0.0000 | -0.5322 | 0.430  | 0.560    | 0.3107  | +0.097    | price_drift        | medium
126   | Down | 45.0000  | 0.6694 | 2.8898 | +0.1121 | -0.3796 | 0.300  | 0.690    | 0.3118  | +0.106    | price_drift        | medium
136   | Up   | 43.0000  | 0.3680 | 5.4247 | +2.5349 | +0.5526 | 0.330  | 0.660    | 0.3118  | +0.062    | unknown            | low 
140   | Down | 15.0000  | 0.6467 | 5.3763 | -0.0484 | +0.5522 | 0.360  | 0.630    | 0.3155  | +0.096    | price_drift        | medium
142   | Up   | 20.0000  | 0.3250 | 5.5302 | +0.1539 | +0.6821 | 0.370  | 0.620    | 0.3155  | +0.083    | price_drift        | medium
152   | Down | 50.0000  | 0.7164 | 1.9997 | -3.5305 | -0.5841 | 0.200  | 0.790    | 0.3158  | +0.098    | signal_flip        | high
152   | Down | 55.0000  | 0.7517 | 1.9997 | +0.0000 | -0.5841 | 0.200  | 0.790    | 0.7144  | -0.054    | price_drift        | medium
162   | Down | 78.0000  | 0.8398 | 1.3146 | -0.6851 | -0.6637 | 0.110  | 0.880    | 0.7175  | -0.010    | price_drift        | medium
168   | Down | 136.0000 | 0.9000 | 0.9608 | -0.3538 | -0.6950 | 0.060  | 0.930    | 0.7305  | +0.046    | price_drift        | medium
172   | Down | 26.2200  | 0.9300 | 0.8392 | -0.1216 | -0.6993 | 0.080  | 0.910    | 0.7571  | +0.130    | price_drift        | medium
172   | Down | 74.1700  | 0.9188 | 0.8392 | +0.0000 | -0.6993 | 0.080  | 0.910    | 0.7622  | +0.145    | price_drift        | medium
172   | Down | 3.7900   | 0.9300 | 0.8392 | +0.0000 | -0.6993 | 0.080  | 0.910    | 0.7742  | +0.184    | price_drift        | medium
172   | Down | 123.9900 | 0.9300 | 0.8392 | +0.0000 | -0.6993 | 0.080  | 0.910    | 0.7748  | +0.186    | unknown            | low 
190   | Up   | 14.0000  | 0.0700 | 0.8904 | +0.0512 | -0.7294 | 0.070  | 0.920    | 0.7924  | +0.243    | price_drift        | medium
190   | Up   | 131.4300 | 0.0672 | 0.8904 | +0.0000 | -0.7294 | 0.070  | 0.920    | 0.7924  | +0.233    | parity_gap         | medium
190   | Up   | 8.5700   | 0.0700 | 0.8904 | +0.0000 | -0.7294 | 0.070  | 0.920    | 0.7924  | +0.148    | parity_gap         | medium
242   | Up   | 211.0000 | 0.0400 | 3.5372 | +2.6468 | +0.4303 | 0.170  | 0.820    | 0.7924  | +0.143    | signal_flip        | high
242   | Up   | 260.0000 | 0.0318 | 3.5372 | +0.0000 | +0.4303 | 0.170  | 0.820    | 0.2224  | -0.030    | avg_down_edge      | high
262   | Up   | 5.0000   | 0.0600 | 4.9651 | +1.4280 | +0.4487 | 0.100  | 0.890    | 0.1840  | +0.082    | price_drift        | medium
262   | Up   | 5.1064   | 0.0600 | 4.9651 | +0.0000 | +0.4487 | 0.100  | 0.890    | 0.1836  | +0.084    | unknown            | low 
268   | Down | 177.0000 | 0.9240 | 4.7782 | -0.1869 | +0.4170 | 0.070  | 0.920    | 0.1831  | +0.086    | parity_gap         | medium
268   | Down | 136.0000 | 0.9200 | 4.7782 | +0.0000 | +0.4170 | 0.070  | 0.920    | 0.1831  | +0.012    | parity_gap         | medium
276   | Up   | 62.5400  | 0.0654 | 4.8952 | +0.1170 | +0.4272 | 0.040  | 0.950    | 0.1831  | -0.039    | unknown            | low 
```

### Emir yorumları

- **t=16s** (Up sz=40.0000 pr=0.4900, `signal_open`): İlk emir — score=5.30, intent=Up; bsi=+0.99, ofi=+0.12
- **t=16s** (Up sz=40.0000 pr=0.4718, `avg_down_edge`): Avg-down: dom avg=0.490, fiyat=0.4718 (1.8 tick aşağı)
- **t=20s** (Up sz=40.0000 pr=0.4951, `price_drift`): Fiyat hareketi → dom requote (price=0.4951)
- **t=24s** (Down sz=25.6900 pr=0.4639, `parity_gap`): Hedge top-up: |dom-opp|=120 share
- **t=42s** (Down sz=9.3300 pr=0.5800, `signal_flip`): Yön değişti: Up→Down (Δscore=-2.94); eski dom emirler iptal edilmiş olmalı
- **t=42s** (Down sz=40.0000 pr=0.4651, `avg_down_edge`): Avg-down: dom avg=0.495, fiyat=0.4651 (3.0 tick aşağı)
- **t=42s** (Down sz=6.6304 pr=0.5400, `price_drift`): Fiyat hareketi → dom requote (price=0.5400)
- **t=42s** (Down sz=23.5600 pr=0.5800, `price_drift`): Fiyat hareketi → dom requote (price=0.5800)
- **t=42s** (Down sz=2.3810 pr=0.5800, `unknown`): Sınıflandırılamadı (review gerekli) — score=2.36, dscore=+0.00, intent_before=Down
- **t=42s** (Down sz=5.7143 pr=0.5800, `unknown`): Sınıflandırılamadı (review gerekli) — score=2.36, dscore=+0.00, intent_before=Down
- **t=68s** (Down sz=65.0000 pr=0.7978, `price_drift`): Fiyat hareketi → dom requote (price=0.7978)
- **t=70s** (Down sz=63.0000 pr=0.7884, `unknown`): Sınıflandırılamadı (review gerekli) — score=0.92, dscore=-0.15, intent_before=Down
- **t=70s** (Down sz=74.0000 pr=0.8088, `price_drift`): Fiyat hareketi → dom requote (price=0.8088)
- **t=76s** (Up sz=71.0000 pr=0.1603, `price_drift`): Fiyat hareketi → hedge requote (price=0.1603)
- **t=82s** (Up sz=3.5256 pr=0.2200, `price_drift`): Fiyat hareketi → hedge requote (price=0.2200)
- **t=84s** (Up sz=53.0000 pr=0.2300, `parity_gap`): Hedge top-up: |dom-opp|=121 share
- **t=84s** (Up sz=52.0000 pr=0.2300, `parity_gap`): Hedge top-up: |dom-opp|=68 share
- **t=86s** (Down sz=17.0000 pr=0.7600, `price_drift`): Fiyat hareketi → dom requote (price=0.7600)
- **t=86s** (Down sz=38.0000 pr=0.7563, `unknown`): Sınıflandırılamadı (review gerekli) — score=1.22, dscore=+0.00, intent_before=Down
- **t=86s** (Down sz=58.0000 pr=0.7700, `price_drift`): Fiyat hareketi → dom requote (price=0.7700)
- **t=88s** (Down sz=60.0000 pr=0.7700, `unknown`): Sınıflandırılamadı (review gerekli) — score=1.13, dscore=-0.09, intent_before=Down
- **t=88s** (Up sz=2.5500 pr=0.2000, `price_drift`): Fiyat hareketi → hedge requote (price=0.2000)
- **t=88s** (Up sz=1.2500 pr=0.2000, `parity_gap`): Hedge top-up: |dom-opp|=186 share
- **t=88s** (Up sz=2.5000 pr=0.2000, `parity_gap`): Hedge top-up: |dom-opp|=185 share
- **t=102s** (Up sz=53.0000 pr=0.2500, `price_drift`): Fiyat hareketi → hedge requote (price=0.2500)
- **t=102s** (Up sz=56.0000 pr=0.2248, `price_drift`): Fiyat hareketi → hedge requote (price=0.2248)
- **t=102s** (Up sz=55.0000 pr=0.2377, `price_drift`): Fiyat hareketi → hedge requote (price=0.2377)
- **t=110s** (Up sz=41.0000 pr=0.4200, `signal_flip`): Yön değişti: Down→Up (Δscore=+1.43); eski dom emirler iptal edilmiş olmalı
- **t=110s** (Up sz=29.8900 pr=0.4000, `price_drift`): Fiyat hareketi → dom requote (price=0.4000)
- **t=110s** (Up sz=20.3900 pr=0.3900, `price_drift`): Fiyat hareketi → dom requote (price=0.3900)
- **t=110s** (Up sz=12.1100 pr=0.4000, `price_drift`): Fiyat hareketi → dom requote (price=0.4000)
- **t=110s** (Up sz=19.5200 pr=0.3800, `price_drift`): Fiyat hareketi → dom requote (price=0.3800)
- **t=110s** (Up sz=11.4300 pr=0.3700, `price_drift`): Fiyat hareketi → dom requote (price=0.3700)
- **t=126s** (Down sz=45.0000 pr=0.6694, `price_drift`): Fiyat hareketi → hedge requote (price=0.6694)
- **t=136s** (Up sz=43.0000 pr=0.3680, `unknown`): Sınıflandırılamadı (review gerekli) — score=5.42, dscore=+2.53, intent_before=Up
- **t=140s** (Down sz=15.0000 pr=0.6467, `price_drift`): Fiyat hareketi → hedge requote (price=0.6467)
- **t=142s** (Up sz=20.0000 pr=0.3250, `price_drift`): Fiyat hareketi → dom requote (price=0.3250)
- **t=152s** (Down sz=50.0000 pr=0.7164, `signal_flip`): Yön değişti: Up→Down (Δscore=-3.53); eski dom emirler iptal edilmiş olmalı
- **t=152s** (Down sz=55.0000 pr=0.7517, `price_drift`): Fiyat hareketi → dom requote (price=0.7517)
- **t=162s** (Down sz=78.0000 pr=0.8398, `price_drift`): Fiyat hareketi → dom requote (price=0.8398)
- **t=168s** (Down sz=136.0000 pr=0.9000, `price_drift`): Fiyat hareketi → dom requote (price=0.9000)
- **t=172s** (Down sz=26.2200 pr=0.9300, `price_drift`): Fiyat hareketi → dom requote (price=0.9300)
- **t=172s** (Down sz=74.1700 pr=0.9188, `price_drift`): Fiyat hareketi → dom requote (price=0.9188)
- **t=172s** (Down sz=3.7900 pr=0.9300, `price_drift`): Fiyat hareketi → dom requote (price=0.9300)
- **t=172s** (Down sz=123.9900 pr=0.9300, `unknown`): Sınıflandırılamadı (review gerekli) — score=0.84, dscore=+0.00, intent_before=Down
- **t=190s** (Up sz=14.0000 pr=0.0700, `price_drift`): Fiyat hareketi → hedge requote (price=0.0700)
- **t=190s** (Up sz=131.4300 pr=0.0672, `parity_gap`): Hedge top-up: |dom-opp|=414 share
- **t=190s** (Up sz=8.5700 pr=0.0700, `parity_gap`): Hedge top-up: |dom-opp|=283 share
- **t=242s** (Up sz=211.0000 pr=0.0400, `signal_flip`): Yön değişti: Down→Up (Δscore=+2.65); eski dom emirler iptal edilmiş olmalı
- **t=242s** (Up sz=260.0000 pr=0.0318, `avg_down_edge`): Avg-down: dom avg=0.222, fiyat=0.0318 (19.1 tick aşağı)
- **t=262s** (Up sz=5.0000 pr=0.0600, `price_drift`): Fiyat hareketi → dom requote (price=0.0600)
- **t=262s** (Up sz=5.1064 pr=0.0600, `unknown`): Sınıflandırılamadı (review gerekli) — score=4.97, dscore=+0.00, intent_before=Up
- **t=268s** (Down sz=177.0000 pr=0.9240, `parity_gap`): Hedge top-up: |dom-opp|=207 share
- **t=268s** (Down sz=136.0000 pr=0.9200, `parity_gap`): Hedge top-up: |dom-opp|=30 share
- **t=276s** (Up sz=62.5400 pr=0.0654, `unknown`): Sınıflandırılamadı (review gerekli) — score=4.90, dscore=+0.12, intent_before=Up

---

## btc-updown-5m-1777468200 — WIN +3563.24 USDC

- 59 emir, son emir t_off=304s
- UP=47, DOWN=12
- Opener: t=12s, Down, score=4.0798, bsi=-0.0222, ofi=-0.2717
- 1 signal_flip:
  - t=106s Down→Up (Δscore=+1.1448)
- Trigger dağılımı: `pre_resolve_scoop`=22, `deadline_cleanup`=15, `price_drift`=13, `parity_gap`=2, `avg_down_edge`=2, `unknown`=2, `signal_open`=1, `pyramid_signal`=1, `signal_flip`=1

### Sinyal trendi (5dk pencere)

```
signal_score [0-10]   : ▄▃▃▄▃▃▃▃▃▃▃▃▃▃▃▃▄▃▃▃▃▃▃▃▃▃▃▃▃▃▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇
up_best_bid  [0-1]    : ▁▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇
down_best_bid [0-1]   : ▁▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▃▃▃▄▄▄▄▃▄▄▄▄▄▄▄▄▄▃▃▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁
ofi          [-1,1]   : ▄▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▅▅▅▄▄▄▄▄▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆
```

### Emir-bazlı detay

```
t_off | outcome | size     | price  | score  | dscore  | ofi     | up_bid | down_bid | avg_dom | imbalance | candidate_trigger  | conf
----- | ---- | -------- | ------ | ------ | ------- | ------- | ------ | -------- | ------- | --------- | ------------------ | ----
12    | Down | 40.0000  | 0.4800 | 4.0798 | +0.0000 | -0.2717 | 0.540  | 0.450    | 0.0000  | +0.000    | signal_open        | high
14    | Up   | 40.0000  | 0.5372 | 4.1965 | +0.1168 | -0.2730 | 0.540  | 0.450    | 0.4800  | +1.000    | parity_gap         | medium
14    | Up   | 41.0000  | 0.5400 | 4.1965 | +0.0000 | -0.2730 | 0.540  | 0.450    | 0.4800  | +0.000    | parity_gap         | low 
54    | Down | 40.0000  | 0.4900 | 6.0908 | +1.8943 | +0.3716 | 0.510  | 0.480    | 0.4800  | -0.339    | price_drift        | medium
64    | Up   | 40.0000  | 0.5248 | 6.2403 | +0.1494 | +0.3349 | 0.570  | 0.420    | 0.4850  | -0.006    | price_drift        | medium
70    | Down | 41.0000  | 0.4215 | 6.5638 | +0.3235 | +0.7575 | 0.540  | 0.450    | 0.4850  | -0.204    | avg_down_edge      | high
70    | Down | 40.0000  | 0.4462 | 6.5638 | +0.0000 | +0.7575 | 0.540  | 0.450    | 0.4635  | +0.000    | pyramid_signal     | medium
72    | Up   | 41.0000  | 0.5463 | 6.5759 | +0.0121 | +0.7624 | 0.560  | 0.430    | 0.4592  | +0.142    | price_drift        | medium
82    | Up   | 42.0000  | 0.5888 | 6.8125 | +0.2366 | +0.8140 | 0.590  | 0.400    | 0.4592  | -0.003    | price_drift        | medium
92    | Down | 40.0000  | 0.4500 | 4.9419 | -1.8706 | -0.0783 | 0.550  | 0.440    | 0.4592  | -0.118    | unknown            | low 
94    | Down | 41.0000  | 0.4200 | 5.0558 | +0.1139 | -0.0786 | 0.560  | 0.430    | 0.4574  | -0.007    | price_drift        | medium
98    | Down | 40.0000  | 0.4303 | 4.8696 | -0.1861 | -0.0763 | 0.560  | 0.430    | 0.4510  | +0.085    | price_drift        | medium
104   | Down | 40.0000  | 0.4500 | 5.6126 | +0.7430 | +0.1830 | 0.690  | 0.290    | 0.4481  | +0.160    | price_drift        | medium
104   | Down | 40.0000  | 0.4438 | 5.6126 | +0.0000 | +0.1830 | 0.690  | 0.290    | 0.4483  | +0.224    | unknown            | low 
106   | Up   | 40.0000  | 0.5200 | 6.7574 | +1.1448 | +0.1912 | 0.730  | 0.260    | 0.4478  | +0.279    | signal_flip        | high
106   | Up   | 41.0000  | 0.5200 | 6.7574 | +0.0000 | +0.1912 | 0.730  | 0.260    | 0.5432  | -0.195    | avg_down_edge      | high
114   | Up   | 63.0000  | 0.7872 | 7.7688 | +1.0114 | +0.2490 | 0.810  | 0.180    | 0.5399  | -0.119    | price_drift        | medium
126   | Up   | 83.0000  | 0.8498 | 7.8439 | +0.0750 | +0.1231 | 0.810  | 0.180    | 0.5847  | -0.020    | price_drift        | medium
126   | Up   | 30.0000  | 0.8300 | 7.8439 | +0.0000 | +0.1231 | 0.810  | 0.180    | 0.6357  | +0.087    | price_drift        | medium
128   | Down | 60.0000  | 0.1995 | 7.6026 | -0.2412 | +0.1182 | 0.820  | 0.170    | 0.6484  | +0.120    | price_drift        | medium
128   | Down | 65.0000  | 0.1700 | 7.6026 | +0.0000 | +0.1182 | 0.820  | 0.170    | 0.6484  | +0.044    | price_drift        | medium
128   | Down | 62.0000  | 0.1806 | 7.6026 | +0.0000 | +0.1182 | 0.820  | 0.170    | 0.6484  | -0.027    | price_drift        | medium
268   | Up   | 2.0800   | 0.9900 | 8.1120 | +0.5094 | +0.1048 | 0.990  | 0.009    | 0.6484  | -0.087    | pre_resolve_scoop  | high
268   | Up   | 5.0000   | 0.9900 | 8.1120 | +0.0000 | +0.1048 | 0.990  | 0.009    | 0.6499  | -0.085    | pre_resolve_scoop  | high
268   | Up   | 1166.5900 | 0.9900 | 8.1120 | +0.0000 | +0.1048 | 0.990  | 0.009    | 0.6535  | -0.080    | pre_resolve_scoop  | high
268   | Up   | 26.0000  | 0.9900 | 8.1120 | +0.0000 | +0.1048 | 0.990  | 0.009    | 0.8937  | +0.497    | pre_resolve_scoop  | high
268   | Up   | 100.0000 | 0.9900 | 8.1120 | +0.0000 | +0.1048 | 0.990  | 0.009    | 0.8952  | +0.503    | pre_resolve_scoop  | high
268   | Up   | 31.4600  | 0.9900 | 8.1120 | +0.0000 | +0.1048 | 0.990  | 0.009    | 0.9005  | +0.525    | pre_resolve_scoop  | high
270   | Up   | 132.0200 | 0.9900 | 8.1358 | +0.0238 | +0.1145 | 0.990  | 0.009    | 0.9021  | +0.531    | pre_resolve_scoop  | high
270   | Up   | 1.0600   | 0.9900 | 8.1358 | +0.0000 | +0.1145 | 0.990  | 0.009    | 0.9081  | +0.556    | pre_resolve_scoop  | high
270   | Up   | 1.1300   | 0.9900 | 8.1358 | +0.0000 | +0.1145 | 0.990  | 0.009    | 0.9082  | +0.556    | pre_resolve_scoop  | high
270   | Up   | 1.0100   | 0.9900 | 8.1358 | +0.0000 | +0.1145 | 0.990  | 0.009    | 0.9082  | +0.556    | pre_resolve_scoop  | high
270   | Up   | 100.0000 | 0.9900 | 8.1358 | +0.0000 | +0.1145 | 0.990  | 0.009    | 0.9083  | +0.557    | pre_resolve_scoop  | high
272   | Up   | 34.1600  | 0.9900 | 9.1217 | +0.9859 | +0.4642 | 0.999  | 0.000    | 0.9123  | +0.574    | pre_resolve_scoop  | high
272   | Up   | 1.1700   | 0.9900 | 9.1217 | +0.0000 | +0.4642 | 0.999  | 0.000    | 0.9136  | +0.579    | pre_resolve_scoop  | high
272   | Up   | 214.1600 | 0.9900 | 9.1217 | +0.0000 | +0.4642 | 0.999  | 0.000    | 0.9136  | +0.580    | pre_resolve_scoop  | high
272   | Up   | 657.0700 | 0.9900 | 9.1217 | +0.0000 | +0.4642 | 0.999  | 0.000    | 0.9208  | +0.611    | pre_resolve_scoop  | high
274   | Up   | 84.3560  | 0.9900 | 9.3477 | +0.2260 | +0.4724 | 0.990  | 0.000    | 0.9363  | +0.685    | pre_resolve_scoop  | high
274   | Up   | 30.4400  | 0.9900 | 9.3477 | +0.0000 | +0.4724 | 0.990  | 0.000    | 0.9378  | +0.692    | pre_resolve_scoop  | high
274   | Up   | 108.1500 | 0.9900 | 9.3477 | +0.0000 | +0.4724 | 0.990  | 0.000    | 0.9383  | +0.695    | pre_resolve_scoop  | high
278   | Up   | 1.9800   | 0.9900 | 9.3480 | +0.0003 | +0.4843 | 0.990  | 0.000    | 0.9401  | +0.704    | pre_resolve_scoop  | high
278   | Up   | 100.0000 | 0.9900 | 9.3480 | +0.0000 | +0.4843 | 0.990  | 0.000    | 0.9401  | +0.704    | pre_resolve_scoop  | high
278   | Up   | 1.0400   | 0.9900 | 9.3480 | +0.0000 | +0.4843 | 0.990  | 0.000    | 0.9417  | +0.712    | pre_resolve_scoop  | high
284   | Up   | 12.0000  | 0.9900 | 9.3672 | +0.0191 | +0.5182 | 0.991  | 0.001    | 0.9417  | +0.712    | pre_resolve_scoop  | high
292   | Up   | 1.0600   | 0.9900 | 9.3756 | +0.0085 | +0.5195 | 0.999  | 0.000    | 0.9419  | +0.713    | deadline_cleanup   | high
292   | Up   | 275.2740 | 0.9900 | 9.3756 | +0.0000 | +0.5195 | 0.999  | 0.000    | 0.9419  | +0.713    | deadline_cleanup   | high
292   | Up   | 1.0600   | 0.9900 | 9.3756 | +0.0000 | +0.5195 | 0.999  | 0.000    | 0.9456  | +0.732    | deadline_cleanup   | high
292   | Up   | 1.0600   | 0.9900 | 9.3756 | +0.0000 | +0.5195 | 0.999  | 0.000    | 0.9456  | +0.732    | deadline_cleanup   | high
292   | Up   | 1.0600   | 0.9900 | 9.3756 | +0.0000 | +0.5195 | 0.999  | 0.000    | 0.9456  | +0.732    | deadline_cleanup   | high
292   | Up   | 1.5000   | 0.9900 | 9.3756 | +0.0000 | +0.5195 | 0.999  | 0.000    | 0.9457  | +0.732    | deadline_cleanup   | high
292   | Up   | 1.0600   | 0.9900 | 9.3756 | +0.0000 | +0.5195 | 0.999  | 0.000    | 0.9457  | +0.732    | deadline_cleanup   | high
292   | Up   | 1.8700   | 0.9900 | 9.3756 | +0.0000 | +0.5195 | 0.999  | 0.000    | 0.9457  | +0.732    | deadline_cleanup   | high
292   | Up   | 1.0600   | 0.9900 | 9.3756 | +0.0000 | +0.5195 | 0.999  | 0.000    | 0.9457  | +0.733    | deadline_cleanup   | high
292   | Up   | 1.0600   | 0.9900 | 9.3756 | +0.0000 | +0.5195 | 0.999  | 0.000    | 0.9457  | +0.733    | deadline_cleanup   | high
292   | Up   | 1.0600   | 0.9900 | 9.3756 | +0.0000 | +0.5195 | 0.999  | 0.000    | 0.9457  | +0.733    | deadline_cleanup   | high
292   | Up   | 1.0600   | 0.9900 | 9.3756 | +0.0000 | +0.5195 | 0.999  | 0.000    | 0.9457  | +0.733    | deadline_cleanup   | high
292   | Up   | 1.0600   | 0.9900 | 9.3756 | +0.0000 | +0.5195 | 0.999  | 0.000    | 0.9458  | +0.733    | deadline_cleanup   | high
292   | Up   | 1.0600   | 0.9900 | 9.3756 | +0.0000 | +0.5195 | 0.999  | 0.000    | 0.9458  | +0.733    | deadline_cleanup   | high
304   | Up   | 1.0600   | 0.9900 | 9.4673 | +0.0917 | +0.6290 | 0.999  | 0.000    | 0.9458  | +0.733    | deadline_cleanup   | high
```

### Emir yorumları

- **t=12s** (Down sz=40.0000 pr=0.4800, `signal_open`): İlk emir — score=4.08, intent=Down; bsi=-0.02, ofi=-0.27
- **t=14s** (Up sz=40.0000 pr=0.5372, `parity_gap`): Hedge top-up: |dom-opp|=40 share
- **t=14s** (Up sz=41.0000 pr=0.5400, `parity_gap`): Hedge top-up: |dom-opp|=0 share
- **t=54s** (Down sz=40.0000 pr=0.4900, `price_drift`): Fiyat hareketi → dom requote (price=0.4900)
- **t=64s** (Up sz=40.0000 pr=0.5248, `price_drift`): Fiyat hareketi → hedge requote (price=0.5248)
- **t=70s** (Down sz=41.0000 pr=0.4215, `avg_down_edge`): Avg-down: dom avg=0.485, fiyat=0.4215 (6.3 tick aşağı)
- **t=70s** (Down sz=40.0000 pr=0.4462, `pyramid_signal`): Pyramid (dom=Down): ofi=0.76, score=6.56 (trend güçlü)
- **t=72s** (Up sz=41.0000 pr=0.5463, `price_drift`): Fiyat hareketi → hedge requote (price=0.5463)
- **t=82s** (Up sz=42.0000 pr=0.5888, `price_drift`): Fiyat hareketi → hedge requote (price=0.5888)
- **t=92s** (Down sz=40.0000 pr=0.4500, `unknown`): Sınıflandırılamadı (review gerekli) — score=4.94, dscore=-1.87, intent_before=Down
- **t=94s** (Down sz=41.0000 pr=0.4200, `price_drift`): Fiyat hareketi → dom requote (price=0.4200)
- **t=98s** (Down sz=40.0000 pr=0.4303, `price_drift`): Fiyat hareketi → dom requote (price=0.4303)
- **t=104s** (Down sz=40.0000 pr=0.4500, `price_drift`): Fiyat hareketi → dom requote (price=0.4500)
- **t=104s** (Down sz=40.0000 pr=0.4438, `unknown`): Sınıflandırılamadı (review gerekli) — score=5.61, dscore=+0.00, intent_before=Down
- **t=106s** (Up sz=40.0000 pr=0.5200, `signal_flip`): Yön değişti: Down→Up (Δscore=+1.14); eski dom emirler iptal edilmiş olmalı
- **t=106s** (Up sz=41.0000 pr=0.5200, `avg_down_edge`): Avg-down: dom avg=0.543, fiyat=0.5200 (2.3 tick aşağı)
- **t=114s** (Up sz=63.0000 pr=0.7872, `price_drift`): Fiyat hareketi → dom requote (price=0.7872)
- **t=126s** (Up sz=83.0000 pr=0.8498, `price_drift`): Fiyat hareketi → dom requote (price=0.8498)
- **t=126s** (Up sz=30.0000 pr=0.8300, `price_drift`): Fiyat hareketi → dom requote (price=0.8300)
- **t=128s** (Down sz=60.0000 pr=0.1995, `price_drift`): Fiyat hareketi → hedge requote (price=0.1995)
- **t=128s** (Down sz=65.0000 pr=0.1700, `price_drift`): Fiyat hareketi → hedge requote (price=0.1700)
- **t=128s** (Down sz=62.0000 pr=0.1806, `price_drift`): Fiyat hareketi → hedge requote (price=0.1806)
- **t=268s** (Up sz=2.0800 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=268s** (Up sz=5.0000 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=268s** (Up sz=1166.5900 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=268s** (Up sz=26.0000 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=268s** (Up sz=100.0000 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=268s** (Up sz=31.4600 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=270s** (Up sz=132.0200 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=270s** (Up sz=1.0600 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=270s** (Up sz=1.1300 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=270s** (Up sz=1.0100 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=270s** (Up sz=100.0000 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=272s** (Up sz=34.1600 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=272s** (Up sz=1.1700 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=272s** (Up sz=214.1600 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=272s** (Up sz=657.0700 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=274s** (Up sz=84.3560 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=274s** (Up sz=30.4400 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=274s** (Up sz=108.1500 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=278s** (Up sz=1.9800 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=278s** (Up sz=100.0000 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=278s** (Up sz=1.0400 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=284s** (Up sz=12.0000 pr=0.9900, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=292s** (Up sz=1.0600 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=292s** (Up sz=275.2740 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=292s** (Up sz=1.0600 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=292s** (Up sz=1.0600 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=292s** (Up sz=1.0600 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=292s** (Up sz=1.5000 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=292s** (Up sz=1.0600 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=292s** (Up sz=1.8700 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=292s** (Up sz=1.0600 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=292s** (Up sz=1.0600 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=292s** (Up sz=1.0600 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=292s** (Up sz=1.0600 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=292s** (Up sz=1.0600 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=292s** (Up sz=1.0600 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=292s, son saniyeler)
- **t=304s** (Up sz=1.0600 pr=0.9900, `deadline_cleanup`): Deadline cleanup (t=304s, son saniyeler)

---

## btc-updown-5m-1777468500 — LOSE / unresolved

- 61 emir, son emir t_off=282s
- UP=37, DOWN=24
- Opener: t=20s, Down, score=6.3215, bsi=+0.2180, ofi=+0.7379
- 2 signal_flip:
  - t=74s Down→Up (Δscore=-1.1698)
  - t=118s Up→Down (Δscore=-1.1746)
- Trigger dağılımı: `price_drift`=27, `parity_gap`=13, `unknown`=13, `pre_resolve_scoop`=3, `signal_flip`=2, `avg_down_edge`=2, `signal_open`=1

### Sinyal trendi (5dk pencere)

```
signal_score [0-10]   : ▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▄▄▄▄▄▄▅▅▅▅▅▅▅▅▄▄▄▃▃▃▂▂▂▂▂▂▂▂
up_best_bid  [0-1]    : ▁▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▅▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▄▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▆▆▇▇▆▇▆▇▇▇▇▇▇▇▇▂▁▁▁▁▂▄▄▄▂▂▁▁▂
down_best_bid [0-1]   : ▁▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▂▂▃▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▅▄▄▄▄▄▄▄▄▄▄▄▄▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▂▂▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▂▂▂▃▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▂▂▂▂▂▂▂▂▂▂▂▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▂▁▁▁▁▁▁▁▁▅▇▇▇▇▆▄▄▄▆▆▆▇▂
ofi          [-1,1]   : ▆▆▆▆▆▆▆▆▆▆▆▇▇▇▆▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▇▆▆▆▆▆▆▆▆▇▇▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▅▅▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▅▅▅▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▆▅▅▅▆▅▅▅▅▅▅▅▅▅▄▄▄▄▄▅▄▅▅▅▅▅▄▄▄▄▄▄▄▄▄▄▅▅▅▅▅▅▄▄▄▄▄▅▄▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▂▂▂▂▂▂▂▂▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▃▄▅▅▅▅▅▅▅▃▃▃▄▄▄▃▃▃▃▃▃▃▃
```

### Emir-bazlı detay

```
t_off | outcome | size     | price  | score  | dscore  | ofi     | up_bid | down_bid | avg_dom | imbalance | candidate_trigger  | conf
----- | ---- | -------- | ------ | ------ | ------- | ------- | ------ | -------- | ------- | --------- | ------------------ | ----
20    | Down | 40.0000  | 0.5025 | 6.3215 | +0.0000 | +0.7379 | 0.520  | 0.470    | 0.0000  | +0.000    | signal_open        | high
24    | Up   | 40.0000  | 0.4957 | 6.5089 | +0.1874 | +0.7235 | 0.580  | 0.410    | 0.5025  | +1.000    | parity_gap         | medium
26    | Up   | 5.6818   | 0.5600 | 7.2716 | +0.7627 | +0.7698 | 0.590  | 0.400    | 0.5025  | +0.000    | price_drift        | medium
28    | Up   | 11.9048  | 0.5800 | 7.2749 | +0.0032 | +0.7968 | 0.610  | 0.380    | 0.5025  | -0.066    | price_drift        | medium
28    | Up   | 2.5000   | 0.5800 | 7.2749 | +0.0000 | +0.7968 | 0.610  | 0.380    | 0.5025  | -0.180    | parity_gap         | medium
28    | Up   | 42.0000  | 0.5900 | 7.2749 | +0.0000 | +0.7968 | 0.610  | 0.380    | 0.5025  | -0.201    | price_drift        | medium
28    | Up   | 2.5714   | 0.5800 | 7.2749 | +0.0000 | +0.7968 | 0.610  | 0.380    | 0.5025  | -0.437    | parity_gap         | medium
28    | Up   | 9.6700   | 0.5800 | 7.2749 | +0.0000 | +0.7968 | 0.610  | 0.380    | 0.5025  | -0.447    | parity_gap         | medium
28    | Up   | 9.3333   | 0.5800 | 7.2749 | +0.0000 | +0.7968 | 0.610  | 0.380    | 0.5025  | -0.482    | parity_gap         | medium
28    | Up   | 5.0000   | 0.5800 | 7.2749 | +0.0000 | +0.7968 | 0.610  | 0.380    | 0.5025  | -0.511    | parity_gap         | medium
28    | Up   | 35.7800  | 0.5384 | 7.2749 | +0.0000 | +0.7968 | 0.610  | 0.380    | 0.5025  | -0.526    | price_drift        | medium
36    | Up   | 46.0000  | 0.6700 | 7.6295 | +0.3546 | +0.7132 | 0.680  | 0.310    | 0.5025  | -0.609    | price_drift        | medium
36    | Up   | 45.0000  | 0.6656 | 7.6295 | +0.0000 | +0.7132 | 0.680  | 0.310    | 0.5025  | -0.681    | parity_gap         | medium
40    | Up   | 49.0000  | 0.6900 | 7.8851 | +0.2557 | +0.7301 | 0.720  | 0.270    | 0.5025  | -0.729    | price_drift        | medium
42    | Up   | 52.0000  | 0.7100 | 7.9954 | +0.1103 | +0.6323 | 0.580  | 0.410    | 0.5025  | -0.768    | price_drift        | medium
74    | Up   | 0.9300   | 0.5200 | 6.8255 | -1.1698 | +0.4807 | 0.570  | 0.420    | 0.5025  | -0.798    | signal_flip        | high
74    | Up   | 40.0000  | 0.4800 | 6.8255 | +0.0000 | +0.4807 | 0.570  | 0.420    | 0.6235  | +0.799    | avg_down_edge      | high
74    | Up   | 40.0000  | 0.4800 | 6.8255 | +0.0000 | +0.4807 | 0.570  | 0.420    | 0.6090  | +0.817    | unknown            | low 
84    | Down | 31.5700  | 0.4579 | 7.1831 | +0.3576 | +0.4977 | 0.510  | 0.480    | 0.5972  | +0.832    | price_drift        | medium
84    | Down | 40.0000  | 0.4500 | 7.1831 | +0.0000 | +0.4977 | 0.510  | 0.480    | 0.5972  | +0.719    | parity_gap         | medium
118   | Down | 35.0000  | 0.4800 | 6.0086 | -1.1746 | +0.4052 | 0.420  | 0.570    | 0.5972  | +0.594    | signal_flip        | high
118   | Down | 20.0000  | 0.4600 | 6.0086 | +0.0000 | +0.4052 | 0.420  | 0.570    | 0.4732  | -0.498    | avg_down_edge      | high
132   | Up   | 19.0000  | 0.4900 | 6.8469 | +0.8383 | +0.4992 | 0.590  | 0.400    | 0.4716  | -0.448    | price_drift        | medium
132   | Up   | 0.7000   | 0.5000 | 6.8469 | +0.0000 | +0.4992 | 0.590  | 0.400    | 0.4716  | -0.465    | price_drift        | medium
140   | Up   | 43.0000  | 0.6300 | 7.2564 | +0.4095 | +0.5187 | 0.630  | 0.360    | 0.4716  | -0.466    | price_drift        | medium
142   | Up   | 44.0000  | 0.6300 | 7.2752 | +0.0188 | +0.5025 | 0.630  | 0.360    | 0.4716  | -0.500    | parity_gap         | medium
142   | Up   | 46.0000  | 0.6300 | 7.2752 | +0.0000 | +0.5025 | 0.630  | 0.360    | 0.4716  | -0.531    | parity_gap         | medium
156   | Up   | 51.0000  | 0.7002 | 7.6036 | +0.3284 | +0.3635 | 0.670  | 0.320    | 0.4716  | -0.560    | price_drift        | medium
158   | Down | 51.0000  | 0.2500 | 7.4092 | -0.1944 | +0.3233 | 0.650  | 0.340    | 0.4716  | -0.588    | price_drift        | medium
166   | Up   | 49.0000  | 0.6772 | 7.3367 | -0.0726 | +0.3419 | 0.700  | 0.290    | 0.4197  | -0.493    | price_drift        | medium
166   | Up   | 46.0000  | 0.6600 | 7.3367 | +0.0000 | +0.3419 | 0.700  | 0.290    | 0.4197  | -0.521    | price_drift        | medium
172   | Down | 44.0000  | 0.3112 | 6.4129 | -0.9238 | +0.0734 | 0.620  | 0.370    | 0.4197  | -0.544    | price_drift        | medium
182   | Up   | 16.1290  | 0.6900 | 6.6245 | +0.2116 | +0.0965 | 0.720  | 0.270    | 0.4014  | -0.476    | price_drift        | medium
194   | Up   | 58.0000  | 0.7600 | 7.1707 | +0.5462 | +0.1630 | 0.820  | 0.170    | 0.4014  | -0.484    | price_drift        | medium
196   | Up   | 11.1100  | 0.7900 | 7.2303 | +0.0597 | +0.1630 | 0.830  | 0.160    | 0.4014  | -0.512    | price_drift        | medium
196   | Up   | 71.0000  | 0.7940 | 7.2303 | +0.0000 | +0.1630 | 0.830  | 0.160    | 0.4014  | -0.517    | parity_gap         | medium
196   | Up   | 5.0000   | 0.7900 | 7.2303 | +0.0000 | +0.1630 | 0.830  | 0.160    | 0.4014  | -0.547    | parity_gap         | medium
196   | Up   | 41.4900  | 0.7900 | 7.2303 | +0.0000 | +0.1630 | 0.830  | 0.160    | 0.4014  | -0.549    | parity_gap         | medium
232   | Down | 12.6000  | 0.1300 | 6.2896 | -0.9407 | -0.2291 | 0.850  | 0.140    | 0.4014  | -0.564    | price_drift        | medium
232   | Down | 8.0000   | 0.1300 | 6.2896 | +0.0000 | -0.2291 | 0.850  | 0.140    | 0.3889  | -0.548    | unknown            | low 
232   | Down | 78.0000  | 0.1400 | 6.2896 | +0.0000 | -0.2291 | 0.850  | 0.140    | 0.3816  | -0.538    | price_drift        | medium
232   | Up   | 154.0000 | 0.9209 | 6.2896 | +0.0000 | -0.2291 | 0.850  | 0.140    | 0.3293  | -0.445    | price_drift        | medium
232   | Down | 1.2529   | 0.1300 | 6.2896 | +0.0000 | -0.2291 | 0.850  | 0.140    | 0.3293  | -0.504    | unknown            | low 
232   | Down | 14.0000  | 0.1300 | 6.2896 | +0.0000 | -0.2291 | 0.850  | 0.140    | 0.3286  | -0.503    | unknown            | low 
232   | Down | 1.1494   | 0.1300 | 6.2896 | +0.0000 | -0.2291 | 0.850  | 0.140    | 0.3212  | -0.489    | unknown            | low 
232   | Down | 22.9885  | 0.1300 | 6.2896 | +0.0000 | -0.2291 | 0.850  | 0.140    | 0.3206  | -0.487    | unknown            | low 
232   | Down | 14.0000  | 0.1300 | 6.2896 | +0.0000 | -0.2291 | 0.850  | 0.140    | 0.3096  | -0.465    | unknown            | low 
232   | Up   | 122.0000 | 0.8956 | 6.2896 | +0.0000 | -0.2291 | 0.850  | 0.140    | 0.3036  | -0.451    | price_drift        | medium
232   | Down | 14.0000  | 0.1300 | 6.2896 | +0.0000 | -0.2291 | 0.850  | 0.140    | 0.3036  | -0.492    | unknown            | low 
242   | Down | 58.0000  | 0.2100 | 5.8642 | -0.4254 | -0.2470 | 0.800  | 0.190    | 0.2979  | -0.479    | price_drift        | medium
256   | Up   | 50.4100  | 0.9400 | 6.0412 | +0.1769 | -0.2708 | 0.940  | 0.050    | 0.2874  | -0.429    | pre_resolve_scoop  | high
264   | Up   | 344.0000 | 0.9634 | 6.0739 | +0.0328 | -0.2994 | 0.970  | 0.020    | 0.2874  | -0.445    | pre_resolve_scoop  | high
264   | Up   | 510.0000 | 0.9733 | 6.0739 | +0.0000 | -0.2994 | 0.970  | 0.020    | 0.2874  | -0.536    | pre_resolve_scoop  | high
274   | Down | 344.0000 | 0.0200 | 5.3918 | -0.6822 | -0.3730 | 0.870  | 0.120    | 0.2874  | -0.627    | price_drift        | medium
282   | Down | 5.7471   | 0.1300 | 7.1350 | +1.7432 | +0.3190 | 0.910  | 0.080    | 0.1765  | -0.437    | price_drift        | medium
282   | Down | 1.1494   | 0.1300 | 7.1350 | +0.0000 | +0.3190 | 0.910  | 0.080    | 0.1762  | -0.435    | unknown            | low 
282   | Up   | 122.0000 | 0.8765 | 7.1350 | +0.0000 | +0.3190 | 0.910  | 0.080    | 0.1761  | -0.434    | price_drift        | medium
282   | Down | 11.4942  | 0.1300 | 7.1350 | +0.0000 | +0.3190 | 0.910  | 0.080    | 0.1761  | -0.456    | unknown            | low 
282   | Down | 35.1200  | 0.1300 | 7.1350 | +0.0000 | +0.3190 | 0.910  | 0.080    | 0.1755  | -0.451    | unknown            | low 
282   | Down | 34.4713  | 0.1300 | 7.1350 | +0.0000 | +0.3190 | 0.910  | 0.080    | 0.1737  | -0.435    | unknown            | low 
282   | Down | 83.0000  | 0.1400 | 7.1350 | +0.0000 | +0.3190 | 0.910  | 0.080    | 0.1720  | -0.419    | unknown            | low 
```

### Emir yorumları

- **t=20s** (Down sz=40.0000 pr=0.5025, `signal_open`): İlk emir — score=6.32, intent=Down; bsi=+0.22, ofi=+0.74
- **t=24s** (Up sz=40.0000 pr=0.4957, `parity_gap`): Hedge top-up: |dom-opp|=40 share
- **t=26s** (Up sz=5.6818 pr=0.5600, `price_drift`): Fiyat hareketi → hedge requote (price=0.5600)
- **t=28s** (Up sz=11.9048 pr=0.5800, `price_drift`): Fiyat hareketi → hedge requote (price=0.5800)
- **t=28s** (Up sz=2.5000 pr=0.5800, `parity_gap`): Hedge top-up: |dom-opp|=18 share
- **t=28s** (Up sz=42.0000 pr=0.5900, `price_drift`): Fiyat hareketi → hedge requote (price=0.5900)
- **t=28s** (Up sz=2.5714 pr=0.5800, `parity_gap`): Hedge top-up: |dom-opp|=62 share
- **t=28s** (Up sz=9.6700 pr=0.5800, `parity_gap`): Hedge top-up: |dom-opp|=65 share
- **t=28s** (Up sz=9.3333 pr=0.5800, `parity_gap`): Hedge top-up: |dom-opp|=74 share
- **t=28s** (Up sz=5.0000 pr=0.5800, `parity_gap`): Hedge top-up: |dom-opp|=84 share
- **t=28s** (Up sz=35.7800 pr=0.5384, `price_drift`): Fiyat hareketi → hedge requote (price=0.5384)
- **t=36s** (Up sz=46.0000 pr=0.6700, `price_drift`): Fiyat hareketi → hedge requote (price=0.6700)
- **t=36s** (Up sz=45.0000 pr=0.6656, `parity_gap`): Hedge top-up: |dom-opp|=170 share
- **t=40s** (Up sz=49.0000 pr=0.6900, `price_drift`): Fiyat hareketi → hedge requote (price=0.6900)
- **t=42s** (Up sz=52.0000 pr=0.7100, `price_drift`): Fiyat hareketi → hedge requote (price=0.7100)
- **t=74s** (Up sz=0.9300 pr=0.5200, `signal_flip`): Yön değişti: Down→Up (Δscore=-1.17); eski dom emirler iptal edilmiş olmalı
- **t=74s** (Up sz=40.0000 pr=0.4800, `avg_down_edge`): Avg-down: dom avg=0.624, fiyat=0.4800 (14.4 tick aşağı)
- **t=74s** (Up sz=40.0000 pr=0.4800, `unknown`): Sınıflandırılamadı (review gerekli) — score=6.83, dscore=+0.00, intent_before=Up
- **t=84s** (Down sz=31.5700 pr=0.4579, `price_drift`): Fiyat hareketi → hedge requote (price=0.4579)
- **t=84s** (Down sz=40.0000 pr=0.4500, `parity_gap`): Hedge top-up: |dom-opp|=366 share
- **t=118s** (Down sz=35.0000 pr=0.4800, `signal_flip`): Yön değişti: Up→Down (Δscore=-1.17); eski dom emirler iptal edilmiş olmalı
- **t=118s** (Down sz=20.0000 pr=0.4600, `avg_down_edge`): Avg-down: dom avg=0.473, fiyat=0.4600 (1.3 tick aşağı)
- **t=132s** (Up sz=19.0000 pr=0.4900, `price_drift`): Fiyat hareketi → hedge requote (price=0.4900)
- **t=132s** (Up sz=0.7000 pr=0.5000, `price_drift`): Fiyat hareketi → hedge requote (price=0.5000)
- **t=140s** (Up sz=43.0000 pr=0.6300, `price_drift`): Fiyat hareketi → hedge requote (price=0.6300)
- **t=142s** (Up sz=44.0000 pr=0.6300, `parity_gap`): Hedge top-up: |dom-opp|=334 share
- **t=142s** (Up sz=46.0000 pr=0.6300, `parity_gap`): Hedge top-up: |dom-opp|=378 share
- **t=156s** (Up sz=51.0000 pr=0.7002, `price_drift`): Fiyat hareketi → hedge requote (price=0.7002)
- **t=158s** (Down sz=51.0000 pr=0.2500, `price_drift`): Fiyat hareketi → dom requote (price=0.2500)
- **t=166s** (Up sz=49.0000 pr=0.6772, `price_drift`): Fiyat hareketi → hedge requote (price=0.6772)
- **t=166s** (Up sz=46.0000 pr=0.6600, `price_drift`): Fiyat hareketi → hedge requote (price=0.6600)
- **t=172s** (Down sz=44.0000 pr=0.3112, `price_drift`): Fiyat hareketi → dom requote (price=0.3112)
- **t=182s** (Up sz=16.1290 pr=0.6900, `price_drift`): Fiyat hareketi → hedge requote (price=0.6900)
- **t=194s** (Up sz=58.0000 pr=0.7600, `price_drift`): Fiyat hareketi → hedge requote (price=0.7600)
- **t=196s** (Up sz=11.1100 pr=0.7900, `price_drift`): Fiyat hareketi → hedge requote (price=0.7900)
- **t=196s** (Up sz=71.0000 pr=0.7940, `parity_gap`): Hedge top-up: |dom-opp|=560 share
- **t=196s** (Up sz=5.0000 pr=0.7900, `parity_gap`): Hedge top-up: |dom-opp|=631 share
- **t=196s** (Up sz=41.4900 pr=0.7900, `parity_gap`): Hedge top-up: |dom-opp|=636 share
- **t=232s** (Down sz=12.6000 pr=0.1300, `price_drift`): Fiyat hareketi → dom requote (price=0.1300)
- **t=232s** (Down sz=8.0000 pr=0.1300, `unknown`): Sınıflandırılamadı (review gerekli) — score=6.29, dscore=+0.00, intent_before=Down
- **t=232s** (Down sz=78.0000 pr=0.1400, `price_drift`): Fiyat hareketi → dom requote (price=0.1400)
- **t=232s** (Up sz=154.0000 pr=0.9209, `price_drift`): Fiyat hareketi → hedge requote (price=0.9209)
- **t=232s** (Down sz=1.2529 pr=0.1300, `unknown`): Sınıflandırılamadı (review gerekli) — score=6.29, dscore=+0.00, intent_before=Down
- **t=232s** (Down sz=14.0000 pr=0.1300, `unknown`): Sınıflandırılamadı (review gerekli) — score=6.29, dscore=+0.00, intent_before=Down
- **t=232s** (Down sz=1.1494 pr=0.1300, `unknown`): Sınıflandırılamadı (review gerekli) — score=6.29, dscore=+0.00, intent_before=Down
- **t=232s** (Down sz=22.9885 pr=0.1300, `unknown`): Sınıflandırılamadı (review gerekli) — score=6.29, dscore=+0.00, intent_before=Down
- **t=232s** (Down sz=14.0000 pr=0.1300, `unknown`): Sınıflandırılamadı (review gerekli) — score=6.29, dscore=+0.00, intent_before=Down
- **t=232s** (Up sz=122.0000 pr=0.8956, `price_drift`): Fiyat hareketi → hedge requote (price=0.8956)
- **t=232s** (Down sz=14.0000 pr=0.1300, `unknown`): Sınıflandırılamadı (review gerekli) — score=6.29, dscore=+0.00, intent_before=Down
- **t=242s** (Down sz=58.0000 pr=0.2100, `price_drift`): Fiyat hareketi → dom requote (price=0.2100)
- **t=256s** (Up sz=50.4100 pr=0.9400, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=264s** (Up sz=344.0000 pr=0.9634, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=264s** (Up sz=510.0000 pr=0.9733, `pre_resolve_scoop`): Pre-resolve scoop: opp_bid çok düşük, kazanan tarafa ucuz hisse
- **t=274s** (Down sz=344.0000 pr=0.0200, `price_drift`): Fiyat hareketi → dom requote (price=0.0200)
- **t=282s** (Down sz=5.7471 pr=0.1300, `price_drift`): Fiyat hareketi → dom requote (price=0.1300)
- **t=282s** (Down sz=1.1494 pr=0.1300, `unknown`): Sınıflandırılamadı (review gerekli) — score=7.13, dscore=+0.00, intent_before=Down
- **t=282s** (Up sz=122.0000 pr=0.8765, `price_drift`): Fiyat hareketi → hedge requote (price=0.8765)
- **t=282s** (Down sz=11.4942 pr=0.1300, `unknown`): Sınıflandırılamadı (review gerekli) — score=7.13, dscore=+0.00, intent_before=Down
- **t=282s** (Down sz=35.1200 pr=0.1300, `unknown`): Sınıflandırılamadı (review gerekli) — score=7.13, dscore=+0.00, intent_before=Down
- **t=282s** (Down sz=34.4713 pr=0.1300, `unknown`): Sınıflandırılamadı (review gerekli) — score=7.13, dscore=+0.00, intent_before=Down
- **t=282s** (Down sz=83.0000 pr=0.1400, `unknown`): Sınıflandırılamadı (review gerekli) — score=7.13, dscore=+0.00, intent_before=Down

---
