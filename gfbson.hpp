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

#include "cursor.hpp"

inline std::string Indent(int depth) {
    return std::string(depth * 2, ' ');
}

enum class NodeType : uint32_t {
    Root        = 300,
    Object      = 301,
    Array       = 302,
    Integer     = 303,

    String      = 305,

    StringTable = 400,

    StringBank  = 500,

    EndOfFile   = 900,
};

struct NodeBase;

struct StringTable;

struct NodeBase {
    virtual ~NodeBase() = default;

    virtual std::string Format(StringTable* pStringTable, int depth = 0) const {
        return std::format("[Node type {}]", mType);
    }

    uint32_t mType;
    uint32_t mContentsSize;
};

class BSON {
public:
    BSON(void* pFile, size_t size)
        : mCursor(pFile)
        , mFilesize(size)
        , mNodes()
    { }

    std::unique_ptr<NodeBase> ReadNode();
    void Parse();
private:
    Cursor mCursor;
    size_t mFilesize;
    std::vector<std::unique_ptr<NodeBase>> mNodes;
};


struct StringInfo {
    void Read(Cursor& rCursor) {
        mStringBankOffset = rCursor.Read32();
        mStringLength = rCursor.Read32();
    }

    uint32_t mStringBankOffset;
    uint32_t mStringLength;
};

struct StringBank final : NodeBase {
    void Read(Cursor& rCursor) {
        mData = rCursor.ReadBytes(mContentsSize);
    }

    std::string Format(StringTable* pStringTable, int depth = 0) const override {
        return ""; // should already be handled by the string table
    }

    std::string GetString(const StringInfo& rInfo) const {
        return std::string(mData.data() + rInfo.mStringBankOffset, rInfo.mStringLength);
    }

    // not the way it's really stored but necessary for c++ rep
    std::vector<char> mData;
};

struct StringTable final : NodeBase {
    void Read(Cursor& rCursor) {
        uint32_t count = mContentsSize / sizeof(StringInfo);
        mStringInfos.reserve(count);
        for (uint32_t i = 0; i < count; i++) {
            StringInfo info;
            info.Read(rCursor);
            mStringInfos.push_back(info);
        }
    }

    std::string Format(StringTable* pStringTable, int depth = 0) const override {
        std::string result = "[String Table]: ";

        for (uint32_t i = 0; i < mStringInfos.size(); i++) {
            result += std::format("\n  [{}] \"{}\"", i, mStringBank->GetString(mStringInfos[i]));
        }

        return result;
    }

    std::string GetString(uint32_t index) const {
        return mStringBank->GetString(mStringInfos[index]);
    }

    // not really stored in-file
    StringBank* mStringBank;
    std::vector<StringInfo> mStringInfos;
};


struct RootNode final : NodeBase {
    std::string Format(StringTable* pStringTable, int depth = 0) const override {
        return "<root>";
    }
};

struct ObjectNode final : NodeBase {
    void Read(BSON* pBSON, Cursor& rCursor) {
        mNameIndex = rCursor.Read32();
        mNumNodes = rCursor.Read32();

        for (uint32_t i = 0; i < mNumNodes; i++) {
            mNodes.push_back(pBSON->ReadNode());
        }
    }

    std::string Format(StringTable* pStringTable, int depth = 0) const override {
        std::string result = std::format("[Object]: Name: \"{}\", Contents: {{", pStringTable->GetString(mNameIndex));

        if (mNodes.empty()) {
            return result + '}';
        }

        for (const std::unique_ptr<NodeBase>& node : mNodes) {
            result += "\n" + Indent(depth + 1) + node->Format(pStringTable, depth + 1);
        }

        return result + "\n" + Indent(depth) + '}';
    }

    uint32_t mNameIndex;
    uint32_t mNumNodes;

    std::vector<std::unique_ptr<NodeBase>> mNodes;
};

struct ArrayNode final : NodeBase {
    void Read(BSON* pBSON, Cursor& rCursor) {
        mNameIndex = rCursor.Read32();
        mNumNodes = rCursor.Read32();

        for (uint32_t i = 0; i < mNumNodes; i++) {
            mNodes.push_back(pBSON->ReadNode());
        }
    }

    std::string Format(StringTable* pStringTable, int depth = 0) const {
        std::string result = std::format("[Array]: Name: \"{}\", Contents: [", pStringTable->GetString(mNameIndex));

        if (mNodes.empty()) {
            return result + '}';
        }

        for (const std::unique_ptr<NodeBase>& node : mNodes) {
            result += "\n" + Indent(depth + 1) + node->Format(pStringTable, depth + 1);
        }

        return result + "\n" + Indent(depth) + ']';
    }

    uint32_t mNameIndex;
    uint32_t mNumNodes;

    // not stored this way but necessary for C++ rep
    std::vector<std::unique_ptr<NodeBase>> mNodes;
};

struct IntegerNode final : NodeBase {
    void Read(Cursor& rCursor) {
        mKeyIndex = rCursor.Read32();
        mValue = static_cast<int32_t>(rCursor.Read32());
    }

    std::string Format(StringTable* pStringTable, int depth = 0) const override {
        return std::format("[Integer]: Key: \"{}\", Value: {}", pStringTable->GetString(mKeyIndex), mValue);
    }

    uint32_t mKeyIndex; // in the string bank
    int32_t mValue;
};

struct StringNode final : NodeBase {
    void Read(Cursor& rCursor) {
        mKeyIndex = rCursor.Read32();
        mValueIndex = rCursor.Read32();
    }

    std::string Format(StringTable* pStringTable, int depth = 0) const override {
        return std::format("[String]: Key: \"{}\", Value: \"{}\"", pStringTable->GetString(mKeyIndex), pStringTable->GetString(mValueIndex));
    }

    uint32_t mKeyIndex; // in the string bank
    uint32_t mValueIndex; // in the string bank
};

struct EOFNode final : NodeBase {
    std::string Format(StringTable* pStringTable, int depth = 0) const override {
        return "<EOF>";
    }
};
