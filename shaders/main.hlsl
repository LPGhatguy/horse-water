struct Vertex {
	float3 pos_in_node;
	float3 normal_in_node;
};

struct ModelNode {
	float4x4 node_to_world;
};

struct View {
	float4x4 world_to_ndc;
};

[[vk::binding(0, 0)]] StructuredBuffer<Vertex> vertices;
[[vk::binding(1, 0)]] StructuredBuffer<ModelNode> nodes;
[[vk::binding(2, 0)]] ConstantBuffer<View> view;

struct VertexInput {
	uint index : SV_InstanceID;
	uint vertex : SV_VertexID;
};

struct VertexOutput {
	precise float4 pos_in_ndc : SV_Position;
};

struct FragOutput {
    [[vk::location(0)]] float4 color : SV_Target0;
};

[shader("vertex")]
VertexOutput vs_main(VertexInput v_in) {
	uint node_id = v_in.index;
	ModelNode node = nodes[node_id];

	Vertex vertex = vertices[v_in.vertex];

	float4 pos_in_world = mul(node.node_to_world, float4(vertex.pos_in_node, 1.0));
	float4 pos_in_ndc = mul(view.world_to_ndc, pos_in_world);

	VertexOutput output;
	output.pos_in_ndc = pos_in_ndc;
	return output;
}

[shader("pixel")]
FragOutput fs_main(VertexOutput v_in) {
	FragOutput output;
	output.color = float4(1.0, 0.0, 0.0, 1.0);
	return output;
}