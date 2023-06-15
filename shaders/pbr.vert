#version 460

//Input
layout(location=0) in vec3 in_pos;
layout(location=1) in vec3 in_normal;
layout(location=2) in vec2 in_texcoords;
layout(location=3) in uint in_material;

//Output
layout(location=0) out vec3 out_pos;
layout(location=1) out vec3 out_normal;
layout(location=2) out vec2 out_texcoords;
layout(location=3) out uint out_material;

//Push constants
layout(push_constant) uniform constants {
	mat4 view;
	mat4 projection;
};

//Descriptors
layout(set=0, binding=0) restrict readonly buffer storage {
	mat4 transformations[];
};

void main() {
	const vec4 pos = vec4(in_pos, 1.0); //Model-space position
	const mat4 t = transformations[gl_DrawID];
	gl_Position = projection * view * t * pos;
	out_pos = vec3(view * t * pos);
	out_normal = normalize(vec3(transpose(inverse(view * t)) * vec4(in_normal, 0.0)));
	out_texcoords = in_texcoords;
	out_material = in_material;
}
