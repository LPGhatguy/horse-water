struct Vertex {
	float3 position;
};

[[vk::binding(0, 0)]] StructuredBuffer<Vertex> vertices;

struct VertexInput {
	uint index : SV_InstanceID;
	uint vertex : SV_VertexID;
};

struct VertexOutput {
	precise float4 position : SV_Position;
};

struct FragOutput {
    [[vk::location(0)]] float4 color : SV_Target0;
};

[shader("vertex")]
VertexOutput vs_main(VertexInput v_in) {
	VertexOutput output;
	output.position = float4(vertices[v_in.vertex].position, 1.0);
	return output;
}

[shader("pixel")]
FragOutput fs_main(VertexOutput v_in) {
	FragOutput output;
	output.color = float4(1.0, 0.0, 0.0, 1.0);
	return output;
}