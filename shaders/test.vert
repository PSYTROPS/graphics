#version 460

//Vertex input
layout(location=0) in vec3 in_pos;
layout(location=1) in vec3 in_color;
//layout(location=2) in vec2 in_tex;
//layout(location=3) in uint in_material;

//Vertex output
layout(location=0) out vec4 out_color;

//Push constants
layout(push_constant) uniform constants {
	mat4 view;
	mat4 projection;
};

//Storage buffer
layout(set=0, binding=0) restrict readonly buffer storage {
	mat4 transformations[];
};

void main() {
	const vec4 pos = vec4(in_pos, 1.0); //Model-space position
	const vec4 color = vec4(in_color, 1.0);
	gl_Position = projection * view * transformations[gl_DrawID] * pos;
	out_color = color;
}
