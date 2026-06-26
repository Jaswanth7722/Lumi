# Lumi Visual Design Guidelines

## Color System

```css
/* Lumi Design Tokens */
--lumi-crystal-default:   #5BC8F5;  /* Primary crystal blue */
--lumi-crystal-thinking:  #5BC8F5;  /* Same as default, pulsing */
--lumi-crystal-success:   #2ECC71;  /* Success green */
--lumi-crystal-error:     #E74C3C;  /* Error red */
--lumi-crystal-warning:   #F5A623;  /* Warning amber */
--lumi-crystal-memory:    #9B59B6;  /* Memory retrieval purple */
--lumi-crystal-learning:  #F1C40F;  /* Learning gold */
--lumi-crystal-sleep:     #BDC3C7;  /* Sleep grey-white */

--lumi-panel-bg:          rgba(20, 24, 32, 0.88);
--lumi-panel-border:      rgba(91, 200, 245, 0.20);
--lumi-panel-text:        #E8EDF2;
--lumi-panel-text-dim:    #8B9BAA;
--lumi-panel-accent:      var(--lumi-crystal-default);
--lumi-panel-success:     #2ECC71;
--lumi-panel-error:       #E74C3C;
--lumi-panel-warning:     #F5A623;
```

## Typography

| Usage | Font | Weight | Size |
|---|---|---|---|
| Panel title | System UI | 600 | 13px |
| Panel body | System UI | 400 | 12px |
| Panel code | JetBrains Mono / monospace | 400 | 11px |
| Panel label | System UI | 500 | 11px |
| Panel caption | System UI | 400 | 10px |

## Animation Timing

| Animation | Duration | Easing |
|---|---|---|
| Panel appear | 220ms | cubic-bezier(0.34, 1.56, 0.64, 1.0) |
| Panel dismiss | 180ms | ease-in |
| Crystal pulse (thinking) | 1200ms | sine wave |
| Emotion transition | 400ms | ease-in-out |
| LOD crossfade | 50ms | linear |
| Character walk start | 320ms | ease-out |

## Icon System

- 16x16 and 24x24 sizes
- 1.5px stroke weight
- Rounded terminals
- Crystal accent color for active/selected states
