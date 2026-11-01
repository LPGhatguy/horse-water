struct Vertex {
	float3 pos_in_node;
	float3 normal_in_node;
};

static const uint VERTEX_OFFSET_POS = 0;
static const uint VERTEX_OFFSET_NORMAL = 12;
static const uint VERTEX_STRIDE = sizeof(Vertex);

struct ModelNode {
	float4x4 world_from_node;
};

struct View {
	float4x4 world_to_ndc;
	float3 camera_in_world;
};

[[vk::binding(0, 0)]] ByteAddressBuffer vertices;
[[vk::binding(1, 0)]] StructuredBuffer<ModelNode> nodes;
[[vk::binding(2, 0)]] ConstantBuffer<View> view;

struct VertexInput {
	uint index : SV_InstanceID;
	uint vertex : SV_VertexID;
};

struct VertexOutput {
	precise float4 pos_in_ndc : SV_Position;
	float3 pos_in_world : TEXCOORD0;
	float3 normal_in_world : NORMAL0;
};

struct FragOutput {
    [[vk::location(0)]] float4 color : SV_Target0;
};

[shader("vertex")]
VertexOutput vs_main(VertexInput v_in) {
	uint node_id = v_in.index;
	ModelNode node = nodes[node_id];

	Vertex vertex;
	vertex.pos_in_node = asfloat(vertices.Load3(v_in.vertex * VERTEX_STRIDE + VERTEX_OFFSET_POS));
	vertex.normal_in_node = asfloat(vertices.Load3(v_in.vertex * VERTEX_STRIDE + VERTEX_OFFSET_NORMAL));

	float4 pos_in_world = mul(node.world_from_node, float4(vertex.pos_in_node, 1.0));
	float4 pos_in_ndc = mul(view.world_to_ndc, pos_in_world);

	float3 normal_in_world = mul(node.world_from_node, float4(vertex.normal_in_node, 0.0)).xyz;

	VertexOutput output;
	output.pos_in_ndc = pos_in_ndc;
	output.pos_in_world = pos_in_world;
	output.normal_in_world = normal_in_world;
	return output;
}

[shader("pixel")]
FragOutput fs_main(VertexOutput p_in) {
	float3 view_dir_in_world = normalize(view.camera_in_world - p_in.pos_in_world);
	float n_dot_v = abs(dot(p_in.normal_in_world, view_dir_in_world));

	float3 base_color = 1.0.xxx;

	FragOutput output;
	output.color = float4(base_color * saturate(pow(n_dot_v, 0.5)), 1.0);
	return output;
}