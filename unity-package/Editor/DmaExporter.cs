using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text;
using UnityEditor;
using UnityEngine;

namespace DesktopMascot.Editor
{
    /// <summary>
    /// Desktop Mascot Avatar (.dma) Exporter for Unity Editor and Batchmode CLI.
    /// Supports Unity 2022.3 LTS, VRCSDK3-AVATAR, lilToon, and VRC PhysBone.
    /// </summary>
    public static class DmaExporter
    {
        private const uint DMA_VERSION = 1;
        private const uint FLAG_LITTLE_ENDIAN = 1 << 1;

        private static readonly byte[] MAGIC_DMA1 = new byte[] { (byte)'D', (byte)'M', (byte)'A', (byte)'1' };
        private static readonly byte[] CHUNK_META = new byte[] { (byte)'M', (byte)'E', (byte)'T', (byte)'A' };
        private static readonly byte[] CHUNK_SKEL = new byte[] { (byte)'S', (byte)'K', (byte)'E', (byte)'L' };
        private static readonly byte[] CHUNK_MESH = new byte[] { (byte)'M', (byte)'E', (byte)'S', (byte)'H' };
        private static readonly byte[] CHUNK_MORPH = new byte[] { (byte)'M', (byte)'O', (byte)'R', (byte)'P' };
        private static readonly byte[] CHUNK_MAT = new byte[] { (byte)'M', (byte)'A', (byte)'T', (byte)' ' };
        private static readonly byte[] CHUNK_PHYS = new byte[] { (byte)'P', (byte)'H', (byte)'Y', (byte)'S' };

        [MenuItem("Tools/Desktop Mascot/Export Active Avatar")]
        public static void ExportActiveAvatarMenuItem()
        {
            GameObject selected = Selection.activeGameObject;
            if (selected == null)
            {
                EditorUtility.DisplayDialog("Desktop Mascot", "Please select an avatar GameObject in the hierarchy.", "OK");
                return;
            }

            string defaultPath = Path.Combine(Application.dataPath, "../", selected.name + ".dma");
            string savePath = EditorUtility.SaveFilePanel("Export .dma Avatar", "", selected.name + ".dma", "dma");
            if (string.IsNullOrEmpty(savePath)) return;

            try
            {
                ExportAvatar(selected, savePath);
                EditorUtility.DisplayDialog("Export Complete", $"Avatar exported successfully to:\n{savePath}", "OK");
            }
            catch (Exception ex)
            {
                Debug.LogError($"[DesktopMascot] Export failed: {ex}");
                EditorUtility.DisplayDialog("Export Error", $"Failed to export avatar:\n{ex.Message}", "OK");
            }
        }

        /// <summary>
        /// Command line batchmode export entrypoint.
        /// Arguments:
        ///   -avatarName <Name>
        ///   -outputPath <Path>
        /// </summary>
        public static void ExportCommandLine()
        {
            string[] args = Environment.GetCommandLineArgs();
            string avatarName = null;
            string outputPath = null;

            for (int i = 0; i < args.Length; i++)
            {
                if (args[i].Equals("-avatarName", StringComparison.OrdinalIgnoreCase) && i + 1 < args.Length)
                {
                    avatarName = args[i + 1];
                }
                else if (args[i].Equals("-outputPath", StringComparison.OrdinalIgnoreCase) && i + 1 < args.Length)
                {
                    outputPath = args[i + 1];
                }
            }

            Debug.Log($"[DesktopMascot] Batchmode export starting: avatarName='{avatarName}', outputPath='{outputPath}'");

            GameObject avatarGo = null;
            if (!string.IsNullOrEmpty(avatarName))
            {
                avatarGo = GameObject.Find(avatarName);
            }

            if (avatarGo == null)
            {
                // Find first object with VRCAvatarDescriptor or Animator
                var descriptor = UnityEngine.Object.FindObjectOfType(GetVrcAvatarDescriptorType());
                if (descriptor != null)
                {
                    avatarGo = ((Component)descriptor).gameObject;
                }
                else
                {
                    var anim = UnityEngine.Object.FindObjectOfType<Animator>();
                    if (anim != null)
                    {
                        avatarGo = anim.gameObject;
                    }
                }
            }

            if (avatarGo == null)
            {
                Debug.LogError("[DesktopMascot] Error: No suitable avatar GameObject found in active scene.");
                EditorApplication.Exit(1);
                return;
            }

            if (string.IsNullOrEmpty(outputPath))
            {
                outputPath = Path.Combine(Directory.GetCurrentDirectory(), avatarGo.name + ".dma");
            }

            try
            {
                ExportAvatar(avatarGo, outputPath);
                Debug.Log($"[DesktopMascot] Export succeeded: {outputPath}");
                EditorApplication.Exit(0);
            }
            catch (Exception ex)
            {
                Debug.LogError($"[DesktopMascot] Export error: {ex}");
                EditorApplication.Exit(2);
            }
        }

        public static void ExportAvatar(GameObject rootGo, string outputPath)
        {
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outputPath)));

            // 1. Gather SkinnedMeshRenderer(s)
            var smrs = rootGo.GetComponentsInChildren<SkinnedMeshRenderer>(true);
            if (smrs.Length == 0)
            {
                throw new InvalidOperationException($"No SkinnedMeshRenderer found on {rootGo.name}");
            }

            // For primary mesh, select the one with most vertices or main body
            SkinnedMeshRenderer primarySmr = smrs[0];
            int maxVerts = 0;
            foreach (var smr in smrs)
            {
                if (smr.sharedMesh != null && smr.sharedMesh.vertexCount > maxVerts)
                {
                    maxVerts = smr.sharedMesh.vertexCount;
                    primarySmr = smr;
                }
            }

            Mesh mesh = primarySmr.sharedMesh;
            Transform[] smrBones = primarySmr.bones;
            Transform rootBone = primarySmr.rootBone != null ? primarySmr.rootBone : rootGo.transform;

            // 2. Build bone hierarchy list
            var boneList = new List<Transform>();
            var boneMap = new Dictionary<Transform, int>();

            void AddTransform(Transform t)
            {
                if (t == null || boneMap.ContainsKey(t)) return;
                if (t.parent != null && t != rootGo.transform && !boneMap.ContainsKey(t.parent))
                {
                    AddTransform(t.parent);
                }
                boneMap[t] = boneList.Count;
                boneList.Add(t);
            }

            AddTransform(rootGo.transform);
            if (smrBones != null)
            {
                foreach (var b in smrBones)
                {
                    if (b != null) AddTransform(b);
                }
            }

            // Also add PhysBone transforms if any
            var physBones = FindPhysBones(rootGo);
            foreach (var pb in physBones)
            {
                var rt = pb.rootTransform != null ? pb.rootTransform : pb.transform;
                AddTransform(rt);
                foreach (Transform child in rt.GetComponentsInChildren<Transform>(true))
                {
                    AddTransform(child);
                }
            }

            var colliders = FindPhysBoneColliders(rootGo);
            foreach (var col in colliders)
            {
                AddTransform(col.transform);
            }

            // 3. Prepare Binary Buffers for Chunks
            byte[] metaChunkData = BuildMetaChunk(rootGo, primarySmr);
            byte[] skelChunkData = BuildSkelChunk(boneList, boneMap, mesh, smrBones, rootGo.transform);
            byte[] meshChunkData = BuildMeshChunk(primarySmr, mesh, boneMap);
            byte[] morphChunkData = BuildMorphChunk(mesh);
            byte[] matChunkData = BuildMaterialChunk(primarySmr.sharedMaterials);
            byte[] physChunkData = BuildPhysChunk(physBones, colliders, boneMap);

            // 4. Assemble .dma container
            using (var fs = new FileStream(outputPath, FileMode.Create, FileAccess.Write, FileShare.None))
            using (var bw = new BinaryWriter(fs))
            {
                var chunks = new List<(byte[] type, byte[] data)>
                {
                    (CHUNK_META, metaChunkData),
                    (CHUNK_SKEL, skelChunkData),
                    (CHUNK_MESH, meshChunkData),
                    (CHUNK_MORPH, morphChunkData),
                    (CHUNK_MAT, matChunkData),
                    (CHUNK_PHYS, physChunkData)
                };

                uint chunkCount = (uint)chunks.Count;

                // Write Header placeholder (32 bytes)
                bw.Write(MAGIC_DMA1);
                bw.Write(DMA_VERSION);
                bw.Write(FLAG_LITTLE_ENDIAN);
                bw.Write(chunkCount);
                bw.Write((ulong)0); // totalFileSize placeholder
                bw.Write(new byte[8]); // reserved

                // Write TOC placeholder (ChunkCount * 24 bytes)
                long tocStart = fs.Position;
                for (int i = 0; i < chunkCount; i++)
                {
                    bw.Write(new byte[24]);
                }

                // Align to 16 bytes for first chunk payload
                PadToAlignment(bw, 16);

                var tocEntries = new List<(byte[] type, ulong offset, ulong length)>();

                // Write each chunk payload
                foreach (var (type, data) in chunks)
                {
                    ulong offset = (ulong)fs.Position;
                    bw.Write(data);
                    ulong length = (ulong)data.Length;
                    tocEntries.Add((type, offset, length));

                    PadToAlignment(bw, 16);
                }

                ulong totalFileSize = (ulong)fs.Position;

                // Seek back and rewrite Header with total size
                fs.Seek(0, SeekOrigin.Start);
                bw.Write(MAGIC_DMA1);
                bw.Write(DMA_VERSION);
                bw.Write(FLAG_LITTLE_ENDIAN);
                bw.Write(chunkCount);
                bw.Write(totalFileSize);
                bw.Write(new byte[8]);

                // Seek to TOC and rewrite TOC entries
                fs.Seek(tocStart, SeekOrigin.Start);
                foreach (var entry in tocEntries)
                {
                    bw.Write(entry.type);
                    bw.Write((uint)0); // reserved
                    bw.Write(entry.offset);
                    bw.Write(entry.length);
                }
            }
        }

        private static void PadToAlignment(BinaryWriter bw, int alignment)
        {
            long pos = bw.BaseStream.Position;
            int pad = (int)((alignment - (pos % alignment)) % alignment);
            for (int i = 0; i < pad; i++)
            {
                bw.Write((byte)0);
            }
        }

        private static byte[] BuildMetaChunk(GameObject rootGo, SkinnedMeshRenderer smr)
        {
            Vector3 viewPos = new Vector3(0, 1.4f, 0.1f);
            var descriptorType = GetVrcAvatarDescriptorType();
            if (descriptorType != null)
            {
                var desc = rootGo.GetComponent(descriptorType);
                if (desc != null)
                {
                    var viewProp = descriptorType.GetField("ViewPosition") ?? descriptorType.GetProperty("ViewPosition") as MemberInfo;
                    if (viewProp is FieldInfo f) viewPos = (Vector3)f.GetValue(desc);
                    else if (viewProp is PropertyInfo p) viewPos = (Vector3)p.GetValue(desc);
                }
            }

            var visemes = new Dictionary<string, int>();
            var blinks = new Dictionary<string, int>();

            if (smr != null && smr.sharedMesh != null)
            {
                for (int i = 0; i < smr.sharedMesh.blendShapeCount; i++)
                {
                    string shapeName = smr.sharedMesh.GetBlendShapeName(i).ToLowerInvariant();
                    if (shapeName.Contains("blink")) blinks[shapeName] = i;
                    if (shapeName.Contains("vrc.v_") || shapeName.StartsWith("v_") || shapeName.Contains("viseme"))
                    {
                        visemes[shapeName] = i;
                    }
                }
            }

            var sb = new StringBuilder();
            sb.Append("{");
            sb.Append($"\"avatar_name\":\"{EscapeJson(rootGo.name)}\",");
            sb.Append($"\"author\":\"UnityExporter\",");
            sb.Append($"\"view_position\":[{viewPos.x.ToString("R")},{viewPos.y.ToString("R")},{viewPos.z.ToString("R")}],");
            sb.Append("\"scale\":1.0,");
            sb.Append("\"viseme_blendshapes\":{");
            int vi = 0;
            foreach (var kvp in visemes)
            {
                if (vi++ > 0) sb.Append(",");
                sb.Append($"\"{EscapeJson(kvp.Key)}\":{kvp.Value}");
            }
            sb.Append("},");
            sb.Append("\"blink_blendshapes\":{");
            int bi = 0;
            foreach (var kvp in blinks)
            {
                if (bi++ > 0) sb.Append(",");
                sb.Append($"\"{EscapeJson(kvp.Key)}\":{kvp.Value}");
            }
            sb.Append("}");
            sb.Append("}");

            return Encoding.UTF8.GetBytes(sb.ToString());
        }

        private static byte[] BuildSkelChunk(
            List<Transform> bones,
            Dictionary<Transform, int> boneMap,
            Mesh mesh,
            Transform[] smrBones,
            Transform rootTransform)
        {
            using (var ms = new MemoryStream())
            using (var bw = new BinaryWriter(ms))
            {
                // Header (8 bytes)
                bw.Write((uint)bones.Count);
                bw.Write((uint)0);

                var bindposes = mesh != null ? mesh.bindposes : null;
                var smrBoneMap = new Dictionary<Transform, int>();
                if (smrBones != null)
                {
                    for (int i = 0; i < smrBones.Length; i++)
                    {
                        if (smrBones[i] != null) smrBoneMap[smrBones[i]] = i;
                    }
                }

                foreach (var bone in bones)
                {
                    // Name [u8; 64]
                    byte[] nameBytes = new byte[64];
                    byte[] strBytes = Encoding.UTF8.GetBytes(bone.name);
                    Array.Copy(strBytes, nameBytes, Math.Min(strBytes.Length, 63));
                    bw.Write(nameBytes);

                    // Parent index (i32)
                    int parentIdx = -1;
                    if (bone.parent != null && boneMap.TryGetValue(bone.parent, out int p))
                    {
                        parentIdx = p;
                    }
                    bw.Write(parentIdx);

                    // Local pos (f32 x 3)
                    Vector3 lpos = bone.localPosition;
                    bw.Write(lpos.x);
                    bw.Write(lpos.y);
                    bw.Write(lpos.z);

                    // Local rot (f32 x 4)
                    Quaternion lrot = bone.localRotation;
                    bw.Write(lrot.x);
                    bw.Write(lrot.y);
                    bw.Write(lrot.z);
                    bw.Write(lrot.w);

                    // Local scale (f32 x 3)
                    Vector3 lscale = bone.localScale;
                    bw.Write(lscale.x);
                    bw.Write(lscale.y);
                    bw.Write(lscale.z);

                    // Inverse bind matrix (f32 x 16, column major)
                    Matrix4x4 invBind = Matrix4x4.identity;
                    if (smrBoneMap.TryGetValue(bone, out int smrIdx) && bindposes != null && smrIdx < bindposes.Length)
                    {
                        invBind = bindposes[smrIdx];
                    }
                    else
                    {
                        invBind = bone.worldToLocalMatrix * rootTransform.localToWorldMatrix;
                    }

                    for (int col = 0; col < 4; col++)
                    {
                        for (int row = 0; row < 4; row++)
                        {
                            bw.Write(invBind[row, col]);
                        }
                    }
                }

                return ms.ToArray();
            }
        }

        private static byte[] BuildMeshChunk(
            SkinnedMeshRenderer smr,
            Mesh mesh,
            Dictionary<Transform, int> boneMap)
        {
            using (var ms = new MemoryStream())
            using (var bw = new BinaryWriter(ms))
            {
                int vertexCount = mesh.vertexCount;
                int submeshCount = mesh.subMeshCount;

                var indicesList = new List<uint>();
                var submeshInfos = new List<(uint offset, uint count, uint matIdx)>();

                for (int s = 0; s < submeshCount; s++)
                {
                    uint offset = (uint)indicesList.Count;
                    var subIndices = mesh.GetIndices(s);
                    foreach (int idx in subIndices)
                    {
                        indicesList.Add((uint)idx);
                    }
                    uint count = (uint)subIndices.Length;
                    uint matIdx = (uint)s;
                    submeshInfos.Add((offset, count, matIdx));
                }

                // Mesh Header (16 bytes)
                bw.Write((uint)vertexCount);
                bw.Write((uint)indicesList.Count);
                bw.Write((uint)submeshCount);
                bw.Write((uint)0);

                // Submesh Infos (16 bytes each)
                foreach (var sub in submeshInfos)
                {
                    bw.Write(sub.offset);
                    bw.Write(sub.count);
                    bw.Write(sub.matIdx);
                    bw.Write((uint)0);
                }

                // Vertex Buffer (80 bytes each)
                var positions = mesh.vertices;
                var normals = mesh.normals;
                var tangents = mesh.tangents;
                var uv0 = mesh.uv;
                var uv1 = mesh.uv2;
                var boneWeights = mesh.boneWeights;
                Transform[] smrBones = smr.bones;

                bool hasNormals = normals != null && normals.Length == vertexCount;
                bool hasTangents = tangents != null && tangents.Length == vertexCount;
                bool hasUv0 = uv0 != null && uv0.Length == vertexCount;
                bool hasUv1 = uv1 != null && uv1.Length == vertexCount;
                bool hasWeights = boneWeights != null && boneWeights.Length == vertexCount;

                for (int v = 0; v < vertexCount; v++)
                {
                    // Position (f32 x 3)
                    Vector3 pos = positions[v];
                    bw.Write(pos.x);
                    bw.Write(pos.y);
                    bw.Write(pos.z);

                    // Normal (f32 x 3)
                    Vector3 norm = hasNormals ? normals[v] : Vector3.up;
                    bw.Write(norm.x);
                    bw.Write(norm.y);
                    bw.Write(norm.z);

                    // Tangent (f32 x 4)
                    Vector4 tan = hasTangents ? tangents[v] : new Vector4(1, 0, 0, 1);
                    bw.Write(tan.x);
                    bw.Write(tan.y);
                    bw.Write(tan.z);
                    bw.Write(tan.w);

                    // UV0 (f32 x 2)
                    Vector2 u0 = hasUv0 ? uv0[v] : Vector2.zero;
                    bw.Write(u0.x);
                    bw.Write(u0.y);

                    // UV1 (f32 x 2)
                    Vector2 u1 = hasUv1 ? uv1[v] : Vector2.zero;
                    bw.Write(u1.x);
                    bw.Write(u1.y);

                    // Bone Indices (u16 x 4) and Weights (f32 x 4)
                    ushort b0 = 0, b1 = 0, b2 = 0, b3 = 0;
                    float w0 = 1, w1 = 0, w2 = 0, w3 = 0;

                    if (hasWeights)
                    {
                        var bwItem = boneWeights[v];
                        ushort MapBone(int smrBoneIdx)
                        {
                            if (smrBones != null && smrBoneIdx >= 0 && smrBoneIdx < smrBones.Length)
                            {
                                var t = smrBones[smrBoneIdx];
                                if (t != null && boneMap.TryGetValue(t, out int mapped)) return (ushort)mapped;
                            }
                            return 0;
                        }

                        b0 = MapBone(bwItem.boneIndex0);
                        b1 = MapBone(bwItem.boneIndex1);
                        b2 = MapBone(bwItem.boneIndex2);
                        b3 = MapBone(bwItem.boneIndex3);

                        w0 = bwItem.weight0;
                        w1 = bwItem.weight1;
                        w2 = bwItem.weight2;
                        w3 = bwItem.weight3;
                    }

                    bw.Write(b0);
                    bw.Write(b1);
                    bw.Write(b2);
                    bw.Write(b3);

                    bw.Write(w0);
                    bw.Write(w1);
                    bw.Write(w2);
                    bw.Write(w3);
                }

                // Index Buffer (u32 each)
                foreach (uint idx in indicesList)
                {
                    bw.Write(idx);
                }

                return ms.ToArray();
            }
        }

        private static byte[] BuildMorphChunk(Mesh mesh)
        {
            using (var ms = new MemoryStream())
            using (var bw = new BinaryWriter(ms))
            {
                int shapeCount = mesh.blendShapeCount;

                // Temporary delta buffers
                int vertCount = mesh.vertexCount;
                var deltaVertices = new Vector3[vertCount];
                var deltaNormals = new Vector3[vertCount];
                var deltaTangents = new Vector3[vertCount];

                var targetEntries = new List<(string name, List<(uint idx, Vector3 dp, Vector3 dn, Vector3 dt)> deltas)>();

                for (int i = 0; i < shapeCount; i++)
                {
                    string shapeName = mesh.GetBlendShapeName(i);
                    mesh.GetBlendShapeFrameVertices(i, 0, deltaVertices, deltaNormals, deltaTangents);

                    var deltas = new List<(uint idx, Vector3 dp, Vector3 dn, Vector3 dt)>();
                    for (int v = 0; v < vertCount; v++)
                    {
                        Vector3 dp = deltaVertices[v];
                        Vector3 dn = deltaNormals[v];
                        Vector3 dt = deltaTangents[v];

                        if (dp.sqrMagnitude > 1e-8f || dn.sqrMagnitude > 1e-8f || dt.sqrMagnitude > 1e-8f)
                        {
                            deltas.Add(((uint)v, dp, dn, dt));
                        }
                    }

                    targetEntries.Add((shapeName, deltas));
                }

                // Header (8 bytes)
                bw.Write((uint)targetEntries.Count);
                bw.Write((uint)0);

                foreach (var entry in targetEntries)
                {
                    // MorphTargetHeader (36 bytes: [u8; 32] + u32)
                    byte[] nameBytes = new byte[32];
                    byte[] strBytes = Encoding.UTF8.GetBytes(entry.name);
                    Array.Copy(strBytes, nameBytes, Math.Min(strBytes.Length, 31));
                    bw.Write(nameBytes);
                    bw.Write((uint)entry.deltas.Count);

                    // DeltaVertex array (40 bytes each)
                    foreach (var d in entry.deltas)
                    {
                        bw.Write(d.idx);
                        bw.Write(d.dp.x);
                        bw.Write(d.dp.y);
                        bw.Write(d.dp.z);
                        bw.Write(d.dn.x);
                        bw.Write(d.dn.y);
                        bw.Write(d.dn.z);
                        bw.Write(d.dt.x);
                        bw.Write(d.dt.y);
                        bw.Write(d.dt.z);
                    }
                }

                return ms.ToArray();
            }
        }

        private static byte[] BuildMaterialChunk(Material[] materials)
        {
            using (var ms = new MemoryStream())
            using (var bw = new BinaryWriter(ms))
            {
                int matCount = materials != null ? materials.Length : 0;
                var textureList = new List<(string name, Texture2D tex)>();
                var texMap = new Dictionary<Texture, int>();

                int RegisterTexture(Texture t)
                {
                    if (t == null) return -1;
                    if (texMap.TryGetValue(t, out int idx)) return idx;
                    if (t is Texture2D t2d)
                    {
                        int newIdx = textureList.Count;
                        textureList.Add((t.name, t2d));
                        texMap[t] = newIdx;
                        return newIdx;
                    }
                    return -1;
                }

                // Collect materials
                var matDatas = new List<(string name, LilToonData data)>();
                if (materials != null)
                {
                    foreach (var mat in materials)
                    {
                        if (mat == null)
                        {
                            matDatas.Add(("DefaultMat", new LilToonData()));
                            continue;
                        }

                        var data = new LilToonData();
                        data.baseColor = mat.HasProperty("_Color") ? mat.GetColor("_Color") : Color.white;
                        data.shadeColor = mat.HasProperty("_ShadowColor") ? mat.GetColor("_ShadowColor") : new Color(0.85f, 0.85f, 0.9f, 1f);
                        data.shade2Color = mat.HasProperty("_Shadow2ndColor") ? mat.GetColor("_Shadow2ndColor") : new Color(0.7f, 0.7f, 0.75f, 1f);
                        data.shadeBorder = mat.HasProperty("_ShadowBorder") ? mat.GetFloat("_ShadowBorder") : 0.5f;
                        data.shadeBlur = mat.HasProperty("_ShadowBlur") ? mat.GetFloat("_ShadowBlur") : 0.1f;
                        data.shade2Border = mat.HasProperty("_Shadow2ndBorder") ? mat.GetFloat("_Shadow2ndBorder") : 0.3f;
                        data.shade2Blur = mat.HasProperty("_Shadow2ndBlur") ? mat.GetFloat("_Shadow2ndBlur") : 0.1f;

                        data.outlineColor = mat.HasProperty("_OutlineColor") ? mat.GetColor("_OutlineColor") : Color.black;
                        data.outlineWidth = mat.HasProperty("_OutlineWidth") ? mat.GetFloat("_OutlineWidth") : 1.0f;
                        data.outlineEnable = (mat.HasProperty("_OutlineWidth") && data.outlineWidth > 0.001f) ? 1u : 0u;
                        data.outlineVertexColorBlend = mat.HasProperty("_OutlineVertexColorBlend") ? mat.GetFloat("_OutlineVertexColorBlend") : 0f;

                        data.rimColor = mat.HasProperty("_RimColor") ? mat.GetColor("_RimColor") : Color.white;
                        data.rimBorder = mat.HasProperty("_RimBorder") ? mat.GetFloat("_RimBorder") : 0.5f;
                        data.rimBlur = mat.HasProperty("_RimBlur") ? mat.GetFloat("_RimBlur") : 0.2f;
                        data.emissionColor = mat.HasProperty("_EmissionColor") ? mat.GetColor("_EmissionColor") : Color.black;

                        if (mat.HasProperty("_MainTex")) data.baseTextureIdx = RegisterTexture(mat.GetTexture("_MainTex"));
                        if (mat.HasProperty("_ShadowColorTex")) data.shadeTextureIdx = RegisterTexture(mat.GetTexture("_ShadowColorTex"));
                        if (mat.HasProperty("_OutlineTex")) data.outlineTextureIdx = RegisterTexture(mat.GetTexture("_OutlineTex"));
                        if (mat.HasProperty("_BumpMap")) data.normalTextureIdx = RegisterTexture(mat.GetTexture("_BumpMap"));

                        matDatas.Add((mat.name, data));
                    }
                }

                // Header (8 bytes)
                bw.Write((uint)matDatas.Count);
                bw.Write((uint)textureList.Count);

                // Write Materials (212 bytes each: 64 name + 148 params)
                foreach (var (mName, mParams) in matDatas)
                {
                    byte[] nameBytes = new byte[64];
                    byte[] strBytes = Encoding.UTF8.GetBytes(mName);
                    Array.Copy(strBytes, nameBytes, Math.Min(strBytes.Length, 63));
                    bw.Write(nameBytes);

                    bw.Write(mParams.baseColor.r);
                    bw.Write(mParams.baseColor.g);
                    bw.Write(mParams.baseColor.b);
                    bw.Write(mParams.baseColor.a);

                    bw.Write(mParams.shadeColor.r);
                    bw.Write(mParams.shadeColor.g);
                    bw.Write(mParams.shadeColor.b);
                    bw.Write(mParams.shadeColor.a);

                    bw.Write(mParams.shade2Color.r);
                    bw.Write(mParams.shade2Color.g);
                    bw.Write(mParams.shade2Color.b);
                    bw.Write(mParams.shade2Color.a);

                    bw.Write(mParams.shadeBorder);
                    bw.Write(mParams.shadeBlur);
                    bw.Write(mParams.shade2Border);
                    bw.Write(mParams.shade2Blur);

                    bw.Write(mParams.outlineColor.r);
                    bw.Write(mParams.outlineColor.g);
                    bw.Write(mParams.outlineColor.b);
                    bw.Write(mParams.outlineColor.a);
                    bw.Write(mParams.outlineWidth);
                    bw.Write(mParams.outlineEnable);
                    bw.Write(mParams.outlineVertexColorBlend);

                    bw.Write(mParams.rimColor.r);
                    bw.Write(mParams.rimColor.g);
                    bw.Write(mParams.rimColor.b);
                    bw.Write(mParams.rimColor.a);
                    bw.Write(mParams.rimBorder);
                    bw.Write(mParams.rimBlur);

                    bw.Write(mParams.emissionColor.r);
                    bw.Write(mParams.emissionColor.g);
                    bw.Write(mParams.emissionColor.b);
                    bw.Write(mParams.emissionColor.a);

                    bw.Write(mParams.baseTextureIdx);
                    bw.Write(mParams.shadeTextureIdx);
                    bw.Write(mParams.outlineTextureIdx);
                    bw.Write(mParams.normalTextureIdx);
                }

                // Write Textures (Header 80 bytes + pixel bytes)
                foreach (var (tName, t2d) in textureList)
                {
                    byte[] nameBytes = new byte[64];
                    byte[] strBytes = Encoding.UTF8.GetBytes(tName);
                    Array.Copy(strBytes, nameBytes, Math.Min(strBytes.Length, 63));
                    bw.Write(nameBytes);

                    byte[] pngBytes = null;
                    try
                    {
                        pngBytes = t2d.isReadable ? t2d.EncodeToPNG() : null;
                    }
                    catch { }

                    if (pngBytes == null) pngBytes = new byte[0];

                    bw.Write((uint)0); // format = PNG
                    bw.Write((uint)t2d.width);
                    bw.Write((uint)t2d.height);
                    bw.Write((uint)pngBytes.Length);

                    if (pngBytes.Length > 0)
                    {
                        bw.Write(pngBytes);
                        int pad = (4 - (pngBytes.Length % 4)) % 4;
                        for (int p = 0; p < pad; p++) bw.Write((byte)0);
                    }
                }

                return ms.ToArray();
            }
        }

        private static byte[] BuildPhysChunk(
            List<PhysBoneWrapper> physBones,
            List<PhysColliderWrapper> colliders,
            Dictionary<Transform, int> boneMap)
        {
            using (var ms = new MemoryStream())
            using (var bw = new BinaryWriter(ms))
            {
                // Header (8 bytes)
                bw.Write((uint)physBones.Count);
                bw.Write((uint)colliders.Count);

                // Build collider map to resolve indices
                var colIndexMap = new Dictionary<PhysColliderWrapper, int>();
                for (int i = 0; i < colliders.Count; i++)
                {
                    colIndexMap[colliders[i]] = i;
                }

                // Write PhysBone Chains (76 bytes each)
                foreach (var pb in physBones)
                {
                    Transform rootT = pb.rootTransform != null ? pb.rootTransform : pb.transform;
                    uint rootBoneIdx = boneMap.TryGetValue(rootT, out int rIdx) ? (uint)rIdx : 0;

                    bw.Write(rootBoneIdx);
                    bw.Write(pb.pull);
                    bw.Write(pb.spring);
                    bw.Write(pb.damping);
                    bw.Write(pb.stiffness);
                    bw.Write(pb.gravity.x);
                    bw.Write(pb.gravity.y);
                    bw.Write(pb.gravity.z);
                    bw.Write(pb.maxAngle * Mathf.Deg2Rad);
                    bw.Write(pb.radius);

                    // Up to 8 collider indices
                    uint[] colIndices = new uint[8];
                    uint activeCount = 0;
                    if (pb.colliderComponents != null)
                    {
                        foreach (var colComp in pb.colliderComponents)
                        {
                            if (colComp == null) continue;
                            var match = colliders.Find(c => c.component == colComp);
                            if (match != null && activeCount < 8)
                            {
                                colIndices[activeCount++] = (uint)colIndexMap[match];
                            }
                        }
                    }

                    for (int i = 0; i < 8; i++) bw.Write(colIndices[i]);
                    bw.Write(activeCount);
                }

                // Write Colliders (52 bytes each)
                foreach (var col in colliders)
                {
                    uint rootBoneIdx = boneMap.TryGetValue(col.transform, out int rIdx) ? (uint)rIdx : 0;
                    bw.Write(col.shape);
                    bw.Write(rootBoneIdx);
                    bw.Write(col.position.x);
                    bw.Write(col.position.y);
                    bw.Write(col.position.z);
                    bw.Write(col.rotation.x);
                    bw.Write(col.rotation.y);
                    bw.Write(col.rotation.z);
                    bw.Write(col.rotation.w);
                    bw.Write(col.radius);
                    bw.Write(col.height);
                    bw.Write(col.insideBounds ? 1u : 0u);
                    bw.Write((uint)0); // reserved
                }

                return ms.ToArray();
            }
        }

        private static List<PhysBoneWrapper> FindPhysBones(GameObject rootGo)
        {
            var list = new List<PhysBoneWrapper>();
            var comps = rootGo.GetComponentsInChildren<Component>(true);
            foreach (var comp in comps)
            {
                if (comp == null) continue;
                var t = comp.GetType();
                if (t.Name.Contains("VRCPhysBone") && !t.Name.Contains("Collider"))
                {
                    list.Add(new PhysBoneWrapper(comp));
                }
            }
            return list;
        }

        private static List<PhysColliderWrapper> FindPhysBoneColliders(GameObject rootGo)
        {
            var list = new List<PhysColliderWrapper>();
            var comps = rootGo.GetComponentsInChildren<Component>(true);
            foreach (var comp in comps)
            {
                if (comp == null) continue;
                var t = comp.GetType();
                if (t.Name.Contains("VRCPhysBoneCollider"))
                {
                    list.Add(new PhysColliderWrapper(comp));
                }
            }
            return list;
        }

        private static Type GetVrcAvatarDescriptorType()
        {
            foreach (var asm in AppDomain.CurrentDomain.GetAssemblies())
            {
                var type = asm.GetType("VRC.SDK3.Avatars.Components.VRCAvatarDescriptor");
                if (type != null) return type;
            }
            return null;
        }

        private static string EscapeJson(string s)
        {
            if (string.IsNullOrEmpty(s)) return "";
            return s.Replace("\\", "\\\\").Replace("\"", "\\\"").Replace("\n", "\\n").Replace("\r", "\\r");
        }

        private class LilToonData
        {
            public Color baseColor = Color.white;
            public Color shadeColor = new Color(0.85f, 0.85f, 0.9f, 1f);
            public Color shade2Color = new Color(0.7f, 0.7f, 0.75f, 1f);
            public float shadeBorder = 0.5f;
            public float shadeBlur = 0.1f;
            public float shade2Border = 0.3f;
            public float shade2Blur = 0.1f;

            public Color outlineColor = Color.black;
            public float outlineWidth = 1.0f;
            public uint outlineEnable = 1;
            public float outlineVertexColorBlend = 0f;

            public Color rimColor = Color.white;
            public float rimBorder = 0.5f;
            public float rimBlur = 0.2f;
            public Color emissionColor = Color.black;

            public int baseTextureIdx = -1;
            public int shadeTextureIdx = -1;
            public int outlineTextureIdx = -1;
            public int normalTextureIdx = -1;
        }

        private class PhysBoneWrapper
        {
            public Component component;
            public Transform transform;
            public Transform rootTransform;
            public float pull = 0.2f;
            public float spring = 0.8f;
            public float damping = 0.1f;
            public float stiffness = 0.0f;
            public Vector3 gravity = new Vector3(0, -9.81f, 0);
            public float maxAngle = 90f;
            public float radius = 0.02f;
            public List<Component> colliderComponents = new List<Component>();

            public PhysBoneWrapper(Component c)
            {
                component = c;
                transform = c.transform;
                var t = c.GetType();

                rootTransform = GetValue<Transform>(t, c, "rootTransform");
                pull = GetValue<float>(t, c, "pull", 0.2f);
                spring = GetValue<float>(t, c, "spring", 0.8f);
                damping = GetValue<float>(t, c, "damping", 0.1f);
                stiffness = GetValue<float>(t, c, "stiffness", 0.0f);
                gravity = GetValue<Vector3>(t, c, "gravity", new Vector3(0, -9.81f, 0));
                maxAngle = GetValue<float>(t, c, "maxAngle", 90f);
                radius = GetValue<float>(t, c, "radius", 0.02f);

                var cols = GetValue<System.Collections.IList>(t, c, "colliders");
                if (cols != null)
                {
                    foreach (var col in cols)
                    {
                        if (col is Component cc) colliderComponents.Add(cc);
                    }
                }
            }

            private static T GetValue<T>(Type t, object obj, string name, T fallback = default)
            {
                var f = t.GetField(name, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
                if (f != null) return (T)Convert.ChangeType(f.GetValue(obj), typeof(T));
                var p = t.GetProperty(name, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
                if (p != null) return (T)Convert.ChangeType(p.GetValue(obj), typeof(T));
                return fallback;
            }
        }

        private class PhysColliderWrapper
        {
            public Component component;
            public Transform transform;
            public uint shape = 0; // 0: Sphere, 1: Capsule, 2: Plane
            public Vector3 position = Vector3.zero;
            public Quaternion rotation = Quaternion.identity;
            public float radius = 0.05f;
            public float height = 0.1f;
            public bool insideBounds = false;

            public PhysColliderWrapper(Component c)
            {
                component = c;
                transform = c.transform;
                var t = c.GetType();

                position = GetValue<Vector3>(t, c, "position", Vector3.zero);
                rotation = GetValue<Quaternion>(t, c, "rotation", Quaternion.identity);
                radius = GetValue<float>(t, c, "radius", 0.05f);
                height = GetValue<float>(t, c, "height", 0.1f);
                insideBounds = GetValue<bool>(t, c, "insideBounds", false);

                var shapeVal = GetValue<object>(t, c, "shape");
                if (shapeVal != null)
                {
                    string sName = shapeVal.ToString();
                    if (sName.Contains("Capsule")) shape = 1;
                    else if (sName.Contains("Plane")) shape = 2;
                    else shape = 0;
                }
            }

            private static T GetValue<T>(Type t, object obj, string name, T fallback = default)
            {
                var f = t.GetField(name, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
                if (f != null)
                {
                    var val = f.GetValue(obj);
                    if (val != null) return (T)val;
                }
                var p = t.GetProperty(name, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
                if (p != null)
                {
                    var val = p.GetValue(obj);
                    if (val != null) return (T)val;
                }
                return fallback;
            }
        }
    }
}
