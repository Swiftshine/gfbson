#pragma once

#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

class Cursor {
public:
    Cursor(void* pPtr)
        : mPtr(reinterpret_cast<uint8_t*>(pPtr))
        , mPos(0)
    { }

    void Seek(size_t num) {
        mPos += num;
    }

    void SeekTo(size_t pos) {
        mPos = pos;
    }

    uint32_t Read32() {
        uint32_t val;
        std::memcpy(&val, mPtr + mPos, sizeof(uint32_t));
        Seek(4);
        return __builtin_bswap32(val);
    }

    std::string ReadString() {
        char* start = reinterpret_cast<char*>(mPtr + mPos);
        size_t len = strlen(start);
        std::string result(start, len);
        Seek(len + 1); // include null terminator
        return result;
    }

    std::vector<char> ReadBytes(size_t num) {
        std::vector<char> result(num);
        std::memcpy(result.data(), mPtr + mPos, num);
        Seek(num);
        return result;
    }

    size_t Pos() const {
        return mPos;
    }

private:
    uint8_t* mPtr;
    size_t mPos;
};
