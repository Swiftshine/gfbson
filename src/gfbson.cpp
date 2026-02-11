#include <cstdlib>
#include <iostream>
#include <string>
#include "gfbson.hpp"

#define UNREACHABLE __builtin_unreachable()

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

    bool tableSet = false;

    while (mCursor.Pos() < mFilesize) {
        mNodes.push_back(ReadNode());

        const auto& node = mNodes.back();

        if (node->mType == static_cast<uint32_t>(NodeType::EndOfFile)) {
            break;
        }

        if (node->mType == static_cast<uint32_t>(NodeType::StringTable)) {
            mStringTable = dynamic_cast<StringTable*>(node.get());
        } else if (node->mType == static_cast<uint32_t>(NodeType::StringBank) && mStringTable != nullptr && !tableSet) {
            mStringTable->mStringBank = dynamic_cast<StringBank*>(node.get());
            tableSet = true;
        }
    }

    // print each node

    // for (const auto& node : mNodes) {
    //     std::cout << node->Format(mStringTable) << std::endl;
    // }
}

void BSON::ProcessNode(json& parent, const std::unique_ptr<NodeBase>& pNode) const {
    switch (static_cast<NodeType>(pNode->mType)) {
        case NodeType::Root:
        case NodeType::StringTable:
        case NodeType::StringBank:
        case NodeType::EndOfFile:
            UNREACHABLE;
            break;

        case NodeType::Object: {
            ObjectNode* node = dynamic_cast<ObjectNode*>(pNode.get());

            json obj = json::object();

            for (const std::unique_ptr<NodeBase>& node : node->mNodes) {
                ProcessNode(obj, node);
            }

            if (parent.is_array()) {
                parent.push_back(std::move(obj));
            } else {
                std::string name = mStringTable->GetString(node->mKeyIndex);
                parent[name] = std::move(obj);
            }
            break;
        }

        case NodeType::Array: {
            ArrayNode* node = dynamic_cast<ArrayNode*>(pNode.get());

            json arr = json::array();

            for (const std::unique_ptr<NodeBase>& node : node->mNodes) {
                ProcessNode(arr, node);
            }

            if (parent.is_array()) {
                parent.push_back(std::move(arr));
            } else {
                std::string name = mStringTable->GetString(node->mKeyIndex);
                parent[name] = std::move(arr);
            }
            break;
        }

        case NodeType::Integer: {
            IntegerNode* node = dynamic_cast<IntegerNode*>(pNode.get());
            if (parent.is_array()) {
                parent.push_back(node->mValue);
            } else {
                std::string name = mStringTable->GetString(node->mKeyIndex);
                parent[name] = node->mValue;
            }
            break;
        }

        case NodeType::String: {
            StringNode* node = dynamic_cast<StringNode*>(pNode.get());
            std::string value = mStringTable->GetString(node->mValueIndex);

            if (parent.is_array()) {
                parent.push_back(value);
            } else {
                std::string name = mStringTable->GetString(node->mKeyIndex);
                parent[name] = std::move(value);
            }

            break;
        }

        default: {
            std::cerr << std::format("ProcessNode: Could not handle node type {}", pNode->mType) << std::endl;
            std::exit(EXIT_FAILURE);
        }
    }
}

std::string BSON::ToJSON() const {
    // root
    json file = json::object();

    for (const std::unique_ptr<NodeBase>& node : mNodes) {
        switch (static_cast<NodeType>(node->mType)) {
            // ignore
            case NodeType::Root:
            case NodeType::StringTable:
            case NodeType::StringBank:
                continue;

            case NodeType::Object:
            case NodeType::Array:
            case NodeType::Integer:
            case NodeType::String: {
                std::string name = mStringTable->GetString(reinterpret_cast<KeyedNode*>(node.get())->mKeyIndex);
                ProcessNode(file[name], node);
                continue;
            }

            case NodeType::EndOfFile:
                return file.dump(4);

            default: {
                std::cerr << std::format("ToJSON: Could not handle node type {}", node->mType) << std::endl;
                std::exit(EXIT_FAILURE);
            }
        }
    }

    return file.dump(4);
}
