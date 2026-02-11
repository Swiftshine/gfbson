// note to self - just rewrite this with polymorphism,
// much less headache that way

#pragma once

#include <cstdint>
#include <variant>
#include <string>
#include <format>
#include <cstring>
#include <vector>
#include <algorithm>
#include <unordered_set>
#include <memory>

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

    size_t Pos() const {
        return mPos;
    }

private:
    uint8_t* mPtr;
    size_t mPos;
};

enum class NodeType : uint32_t {
    Root        = 300,
    Object      = 301,
    Array       = 302,
    Integer     = 303,

    String      = 305,

    StringBank  = 500,

    EndOfFile   = 900,
};

struct StringBank;

struct NodeBase {
    virtual ~NodeBase() = default;

    std::string GetType() const {
        return std::format("Node type {}", mType);
    }

    std::string Format(StringBank* pStringBank) const {
        return "<unk>";
    }

    uint32_t mType;
    uint32_t mContentsSize;
};

struct StringBank final : NodeBase {
    void Read(Cursor& rCursor) {
        size_t end = rCursor.Pos() + mContentsSize;
        
        while (rCursor.Pos() < end) {
            mStrings.push_back(rCursor.ReadString());
        }
        
        // remove redundant strings
        std::unordered_set<std::string> seen;
        mStrings.erase(std::remove_if(
            mStrings.begin(),
            mStrings.end(),
            [&seen](std::string s) {
                return !seen.insert(s).second;
            }
        ), mStrings.end());
    }

    std::string GetType() const {
        return "String Bank";
    }

    std::string Format(StringBank* pStringBank) const {
        std::string result = "\n";
        
        for (size_t i = 0; i < mStrings.size(); i++) {
            result += std::format("\t[{}] {}\n", i, mStrings[i]);
        }

        return result;
    }

    const std::string& GetString(size_t index) const {
        return mStrings[index];
    }

    // not the way it's really stored but necessary for c++ rep
    std::vector<std::string> mStrings;
};


struct RootNode final : NodeBase {
    std::string GetType() const {
        return "Root";
    }

    std::string Format(StringBank* pStringBank) const {
        return "<root>";
    }
};

struct ObjectNode final : NodeBase {
    void Read(Cursor& rCursor) {
        mNameIndex = rCursor.Read32();
        mNumNodes = rCursor.Read32();
    }

    std::string GetType() const {
        return "Object";
    }

    std::string Format(StringBank* pStringBank) const {
        return std::format("Name: \"{}\", # of Nodes: {}", pStringBank->GetString(mNameIndex), mNumNodes);
    }

    uint32_t mNameIndex;
    uint32_t mNumNodes;
};

// struct ArrayNode final : NodeBase {
//     void Read(BSON* pBSON, Cursor& rCursor) {
//         mNameIndex = rCursor.Read32();
//         mNumNodes = rCursor.Read32();

//         for (uint32_t i = 0; i < mNumNodes; i++) {
//             Node node = pBSON->ReadNode();
//             mNodes.push_back(std::make_unique<Node>(std::move(node)));
//         }
//     }

//     std::string GetType() const {
//         return "Array";
//     }

//     std::string Format(StringBank* pStringBank) const {
//         std::string base = std::format("Name: \"{}\", # of Nodes: {}", 
//                                    pStringBank->GetString(mNameIndex), mNumNodes);
//         for (const auto& childPtr : mNodes) {
//             std::visit([pStringBank, &base](const auto& arg) {
//                 base += "\n\t  -> " + arg.GetType();
//             }, *childPtr);
//         }
//         return base;
//     }

//     uint32_t mNameIndex;
//     uint32_t mNumNodes;

//     // not really stored this way but necessary for c++ rep
//     std::vector<std::unique_ptr<Node>> mNodes; // unique ptrs to avoid recursive variant issues
// };

struct IntegerNode final : NodeBase {
    void Read(Cursor& rCursor) {
        mKeyIndex = rCursor.Read32();
        mValue = static_cast<int32_t>(rCursor.Read32());
    }

    std::string GetType() const {
        return "Integer";
    }

    std::string Format(StringBank* pStringBank) const {
        return std::format("Key: \"{}\", Value: \"{}\"", pStringBank->GetString(mKeyIndex), mValue);
    }

    uint32_t mKeyIndex; // in the string bank
    int32_t mValue;
};

struct StringNode final : NodeBase {
    void Read(Cursor& rCursor) {
        mKeyIndex = rCursor.Read32();
        mValueIndex = rCursor.Read32();
    }

    std::string GetType() const {
        return "String";
    }

    std::string Format(StringBank* pStringBank) const {
        return std::format("Key: \"{}\", Value: \"{}\"", pStringBank->GetString(mKeyIndex), pStringBank->GetString(mValueIndex));
    }

    uint32_t mKeyIndex; // in the string bank
    uint32_t mValueIndex; // in the string bank
};

// define Node for real
using Node = std::variant<
    NodeBase,
    RootNode,
    ObjectNode,
    IntegerNode,
    StringNode,
    StringBank
    // ArrayNode
>;


class BSON {
public:
    BSON(void* pFile, size_t size)
        : mCursor(pFile)
        , mFilesize(size)
        , mNodes()
        , mStrings()
    { }

    Node ReadNode();
    void Parse();
private:
    Cursor mCursor;
    size_t mFilesize;
    std::vector<Node> mNodes;
    std::vector<std::string> mStrings;
};
