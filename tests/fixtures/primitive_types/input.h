#pragma once

#include <cstdint>

void takes_fixed(std::int8_t a, std::uint16_t b, std::int32_t c, std::uint64_t d);
void takes_plain(char a, signed char b, unsigned char c, short d, unsigned int e, long long f);
void takes_pointers(const int* in, float* out, const char* name);
void takes_refs(const double& in, int& out, std::int64_t&& moved);

// typedef zinciri: my_id -> std::int32_t
typedef std::int32_t my_id;
my_id next_id(my_id current);
