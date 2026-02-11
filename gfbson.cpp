#include <iostream>
#include "gfbson.hpp"

std::unique_ptr<NodeBase> BSON::ReadNode() {
    uint32_t type = mCursor.Read32();
    uint32_t size = mCursor.Read32();

    switch (static_cast<NodeType>(type)) {
        case NodeType::Root: {
            RootNode node;
            node.mType = type;
            node.mContentsSize = size;
            return std::make_unique<RootNode>(node);
        }

        case NodeType::Object: {
            ObjectNode node;
            node.mType = type;
            node.mContentsSize = size;
            node.Read(this, mCursor);
            return std::make_unique<ObjectNode>(std::move(node));
        }

        case NodeType::Array: {
            ArrayNode node;
            node.mType = type;
            node.mContentsSize = size;
            node.Read(this, mCursor);
            return std::make_unique<ArrayNode>(std::move(node));
        }

        case NodeType::Integer: {
            IntegerNode node;
            node.mType = type;
            node.mContentsSize = size;
            node.Read(mCursor);
            return std::make_unique<IntegerNode>(node);
        }

        case NodeType::String: {
            StringNode node;
            node.mType = type;
            node.mContentsSize = size;
            node.Read(mCursor);
            return std::make_unique<StringNode>(node);
        }

        case NodeType::StringTable: {
            StringTable node;
            node.mType = type;
            node.mContentsSize = size;
            node.Read(mCursor);
            return std::make_unique<StringTable>(node);
        };

        case NodeType::StringBank: {
            StringBank node;
            node.mType = type;
            node.mContentsSize = size;
            node.Read(mCursor);
            return std::make_unique<StringBank>(node);
        };

        case NodeType::EndOfFile: {
            EOFNode node;
            node.mType = type;
            node.mContentsSize = size;
            return std::make_unique<EOFNode>(node);
        }

        default: {
            NodeBase node;
            node.mType = type;
            node.mContentsSize = size;

            // skip unk bytes
            mCursor.Seek(size);

            return std::make_unique<NodeBase>(node);
        }
    }
}

void BSON::Parse() {
    // skip header
    mCursor.SeekTo(0x10);

    StringTable* stringTable = nullptr;
    bool tableSet = false;

    while (mCursor.Pos() < mFilesize) {
        mNodes.push_back(ReadNode());

        const auto& node = mNodes.back();

        if (node->mType == static_cast<uint32_t>(NodeType::EndOfFile)) {
            break;
        }


        if (node->mType == static_cast<uint32_t>(NodeType::StringTable)) {
            stringTable = dynamic_cast<StringTable*>(node.get());
        } else if (node->mType == static_cast<uint32_t>(NodeType::StringBank) && stringTable != nullptr && !tableSet) {
            stringTable->mStringBank = dynamic_cast<StringBank*>(node.get());
            tableSet = true;
        }
    }

    // print each node

    for (const auto& node : mNodes) {
        std::cout << node->Format(stringTable) << std::endl;
    }
}
