# レンダリング＆シェーダー設計書 (Rendering & Shader Specification)

## 1. レンダリングアーキテクチャ概要

デスクトップマスコットの描画系は、クロスプラットフォームかつ低オーバーヘッドなグラフィックスAPIラッパーである **`wgpu`** を基盤とし、シェーディング言語には **WGSL (WebGPU Shading Language)** を採用する。

Windows環境において、透過ウィンドウ（DWM合成）上でアバターを美しく表示するためには、**ストレートアルファ**ではなく**乗算済みアルファ（Pre-Multiplied Alpha）**による正確なアルファブレンディングが不可欠である。さらに、セルルック（Toon調）アバターの美麗さを決定づける **lilToonシェーダーの完全再現** と、輪郭線を生成する **Inverted Hull（裏面押し出し）法** をマルチパス描画で実現する。

```mermaid
flowchart TD
    subgraph GPU Render Passes
        subgraph Pass1 [Pass 1: メインToonシェーディング]
            P1_Depth[Depth Write: ON / Less]
            P1_Cull[Cull Face: Back]
            P1_Shade[lilToon WGSL<br/>Base + 2段階影 + リムライト + 発光]
        end
        subgraph Pass2 [Pass 2: Inverted Hull アウトライン]
            P2_Depth[Depth Write: ON / LessEqual]
            P2_Cull[Cull Face: Front (裏面描画)]
            P2_Extrude[法線方向押し出し<br/>スクリーン空間一定太さ補正]
        end
        subgraph Pass3 [Pass 3: UI & 吹き出し描画 (egui)]
            P3_Overlay[egui-wgpu レンダーパス<br/>テキスト / アイコン / ボタン]
        end
    end
    Pass1 --> Pass2 --> Pass3 --> Present[Swapchain Present<br/>PreMultiplied Alpha合成]
```

---

## 2. 透過サーフェスとブレンド設定

### 2.1 wgpu Surface 設定
Windows DWMと透過レイヤードウィンドウで背景が黒ずんだり境界がフリンジ化するのを防止するため、パイプラインのピクセルフォーマットと合成モードを厳密に設定する。

- **TextureFormat**: `TextureFormat::Bgra8UnormSrgb` または `Rgba8UnormSrgb`
- **CompositeAlphaMode**: `CompositeAlphaMode::PreMultiplied` (OSコンポジタでの加算・ブレンドを整合させる)
- **PresentMode**: `PresentMode::AutoVsync` (通常時) / 省電力時はフレームスキップ制御

### 2.2 ブレンドステート (Pre-Multiplied Alpha)
```rust
pub const PREMULTIPLIED_ALPHA_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
};
```

---

## 3. lilToon WGSL シェーダー実装仕様

### 3.1 陰影計算モデル (2段階Toonステップ)
Half-Lambert または 標準Lambert 内積 $\cos\theta = \vec{N} \cdot \vec{L}$ をベースに、lilToonの `shade_border` (境界位置) と `shade_blur` (境界の滑らかさ) を用いて `smoothstep` で階段状の階調を生成する。

$$T_1 = \text{smoothstep}(\text{border}_1 - \text{blur}_1, \text{border}_1 + \text{blur}_1, \vec{N} \cdot \vec{L})$$
$$T_2 = \text{smoothstep}(\text{border}_2 - \text{blur}_2, \text{border}_2 + \text{blur}_2, \vec{N} \cdot \vec{L})$$

カラー合成:
$$C_{\text{final}} = \text{lerp}(C_{\text{shade2}}, \text{lerp}(C_{\text{shade1}}, C_{\text{base}}, T_1), T_2)$$

### 3.2 WGSL シェーダーコード骨格

```wgsl
struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_pos: vec3<f32>,
    fov_factor: f32,
};

struct LightUniform {
    direction: vec3<f32>,
    color: vec3<f32>,
    ambient_color: vec3<f32>,
};

struct MaterialUniform {
    base_color: vec4<f32>,
    shade_color: vec4<f32>,
    shade2_color: vec4<f32>,
    shade_border: f32,
    shade_blur: f32,
    shade2_border: f32,
    shade2_blur: f32,
    rim_color: vec4<f32>,
    rim_border: f32,
    rim_blur: f32,
    emission_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> light: LightUniform;
@group(1) @binding(0) var<uniform> mat: MaterialUniform;
@group(1) @binding(1) var t_base: texture_2d<f32>;
@group(1) @binding(2) var s_base: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) bone_indices: vec4<u32>,
    @location(5) bone_weights: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // スキニング計算 (行列パレット適用)
    let skin_matrix = get_skin_matrix(in.bone_indices, in.bone_weights);
    let world_pos = skin_matrix * vec4<f32>(in.position, 1.0);
    let world_norm = normalize((skin_matrix * vec4<f32>(in.normal, 0.0)).xyz);
    
    out.clip_position = camera.view_proj * world_pos;
    out.world_pos = world_pos.xyz;
    out.world_normal = world_norm;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let base_tex = textureSample(t_base, s_base, in.uv) * mat.base_color;
    let n = normalize(in.world_normal);
    let l = normalize(light.direction);
    let v = normalize(camera.view_pos - in.world_pos);
    
    // Half-Lambert 陰影
    let n_dot_l = dot(n, l) * 0.5 + 0.5;
    let shade_factor1 = smoothstep(mat.shade_border - mat.shade_blur, mat.shade_border + mat.shade_blur, n_dot_l);
    let shade_factor2 = smoothstep(mat.shade2_border - mat.shade2_blur, mat.shade2_border + mat.shade2_blur, n_dot_l);
    
    let shaded_col = mix(mat.shade2_color.rgb, mix(mat.shade1_color.rgb, base_tex.rgb, shade_factor1), shade_factor2);
    
    // リムライト計算 (フレネル効果)
    let fresnel = 1.0 - max(dot(v, n), 0.0);
    let rim_factor = smoothstep(mat.rim_border - mat.rim_blur, mat.rim_border + mat.rim_blur, fresnel);
    let rim = mat.rim_color.rgb * rim_factor;
    
    // 発光
    let emission = mat.emission_color.rgb;
    
    let final_rgb = shaded_col * light.color + light.ambient_color * base_tex.rgb + rim + emission;
    let final_alpha = base_tex.a;
    
    // 乗算済みアルファ出力
    return vec4<f32>(final_rgb * final_alpha, final_alpha);
}
```

---

## 4. Inverted Hull (裏面押し出し) アウトライン実装

### 4.1 アルゴリズム
1. メッシュの**裏面（Backface）のみを描画**（`CullMode::Front`）。
2. 頂点シェーダーにおいて、各頂点を**法線方向（または事前にベイクされた押し出しベクトル方向）へ拡大**。
3. 遠近法（Perspective）によってカメラから遠い頂点のアウトラインが細くなったり太くなったりしないよう、**ビュー空間深度 $z$ とFOVに応じたスケール補正** を適用。

### 4.2 アウトライン頂点シェーダー (WGSL)
```wgsl
struct OutlineUniform {
    color: vec4<f32>,
    width: f32, // モデル空間基準幅 (例: 0.0015)
};

@group(2) @binding(0) var<uniform> outline: OutlineUniform;

@vertex
fn vs_outline(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let skin_matrix = get_skin_matrix(in.bone_indices, in.bone_weights);
    let world_pos = (skin_matrix * vec4<f32>(in.position, 1.0)).xyz;
    let world_norm = normalize((skin_matrix * vec4<f32>(in.normal, 0.0)).xyz);
    
    // クリップ空間へ変換
    var clip_pos = camera.view_proj * vec4<f32>(world_pos, 1.0);
    let clip_norm = camera.view_proj * vec4<f32>(world_norm, 0.0);
    
    // スクリーン空間での太さを維持するための補正係数 (深度 clip_pos.w に比例)
    let offset = normalize(clip_norm.xy) * outline.width * clip_pos.w * 0.002;
    clip_pos.x += offset.x;
    clip_pos.y += offset.y;
    
    // Zファイティング防止のための深度微小バイアス
    clip_pos.z += 0.0001 * clip_pos.w;
    
    out.clip_position = clip_pos;
    out.world_pos = world_pos;
    out.world_normal = world_norm;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_outline(in: VertexOutput) -> @location(0) vec4<f32> {
    let col = outline.color;
    // 乗算済みアルファ
    return vec4<f32>(col.rgb * col.a, col.a);
}
```

---

## 5. GPUスキニング & ブレンドシェイプ

### 5.1 リニアスキニング (Linear Blend Skinning)
- 頂点ごとに最大4ボーンの影響度（`bone_indices: [u16; 4]`, `bone_weights: [f32; 4]`）をサポート。
- ボーン変形行列パレットは Storage Buffer (`array<mat4x4<f32>>`) に保持し、毎フレームCPUの物理・アニメーション更新結果を1回の一括コピー（`queue.write_buffer`）でGPUへ転送。

### 5.2 ブレンドシェイプ適用アーキテクチャ
- **方式**: Compute Shader による頂点バッファ前処理、または頂点シェーダー内での動的合成。
- 本ランタイムでは省電力性を重視し、**Compute Shader による差分適用（Morph Compute Pass）** を採用。有効な表情（ウェイト > 0.0）のターゲットのみを合算して一次頂点バッファへ書き込み、その後のRenderPassで共通利用する。

---

## 6. 省電力・リフレッシュレート最適化

常駐型デスクトップマスコットとして、GPUファンを回さずバッテリー消費を抑えるための設計。

1. **アダプティブ・フレームレート (可変FPS)**:
   - **アクティブ時 (マウスホバー、ドラッグ、会話発話中)**: 60 FPS
   - **通常アイドル時 (呼吸、まばたき、PhysBone微振動)**: 30 FPS
   - **バックグラウンド待機時 (他アプリ全画面作業中など)**: 15 FPS
2. **ダーティ領域判定 & レンダリング抑制**:
   - アニメーションや物理挙動、マウスインタラクションが完全に静止しているフレームでは再描画をスキップし、スワップチェーンのPresentのみを行う（消費電力 < 1W を目指す）。
