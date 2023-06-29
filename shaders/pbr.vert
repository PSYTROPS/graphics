#version 460

//Input
layout(location=0) in vec3 in_pos;
layout(location=1) in vec3 in_normal;
layout(location=2) in vec2 in_texcoords;

//Output
layout(location=0) out vec3 out_pos;
layout(location=1) out vec3 out_normal;
layout(location=2) out vec2 out_texcoords;
layout(location=3) out uint out_material;

//Push constants
layout(push_constant) uniform constants {
	mat4 view;
	mat4 projection;
	vec4 camera_pos;
};

//Descriptors
struct Primitive {
	uint material;
};
layout(std140, set=0, binding=0) restrict readonly buffer primitive_storage {
	Primitive primitives[];
};
struct Node {
	mat4 transform;
	mat4 inverse_transform;
	uint mesh;
	uint flags;
};
layout(std140, set=0, binding=1) restrict readonly buffer node_storage {
	Node nodes[];
};
struct Extra {
	uint node;
	uint primitive;
};
layout(std140, set=0, binding=2) restrict readonly buffer extra_storage {
	Extra extras[];
};

void main() {
	//Inputs
	const Extra extra = extras[gl_DrawID];
	const Node node = nodes[extra.node];
	const Primitive primitive = primitives[extra.primitive];
	//Position
	const vec4 pos = vec4(in_pos, 1.0); //Model-space position
	const vec4 world_pos = node.transform * pos;
	gl_Position = projection * view * world_pos;
	//Outputs
	out_pos = vec3(world_pos);
	out_normal = normalize(vec3(transpose(node.inverse_transform) * vec4(in_normal, 0.0)));
	out_texcoords = in_texcoords;
	out_material = primitive.material;
}
