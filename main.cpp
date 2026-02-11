#include <iostream>
#include <fstream>
#include <stdlib.h>
#include <vector>
#include <iomanip>

#include "gfbson.h"

int main(int argc, char* argv[]) {
    if (argc != 2) {
        return EXIT_FAILURE;
    }

    // read file
    std::ifstream in(argv[1], std::ios::in | std::ios::binary);
    std::vector<char> file((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
    in.close();

    BSON bson(file.data(), file.size());
    bson.Parse();

    return EXIT_SUCCESS;
}
