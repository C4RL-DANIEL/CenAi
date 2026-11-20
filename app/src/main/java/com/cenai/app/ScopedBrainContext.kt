package com.cenai.app

/**
 * A feature-owned view into the shared CenAI Brain. It contains no cross-user
 * mutable state: all persisted reads and writes are bound to one principal and
 * one feature namespace at construction time.
 */
class ScopedBrainContext internal constructor(
    val scope: BrainContextScope,
    private val vault: SecureKeyStore,
    val chatStore: ChatStore,
    private val knowledgeStore: LocalKnowledgeStore
) {
    fun loadConversations(): MutableList<Conversation> = chatStore.load()

    fun saveConversations(conversations: List<Conversation>) = chatStore.save(conversations)

    fun getPreference(name: String, fallback: String = ""): String =
        vault.getScoped(scope, scope.featureId, name, fallback)

    fun putPreference(name: String, value: String) =
        vault.putScoped(scope, scope.featureId, name, value)

    fun getPrivateMemory(name: String): String =
        vault.getSecretScoped(scope, scope.featureId, name)

    fun putPrivateMemory(name: String, value: String) =
        vault.putSecretScoped(scope, scope.featureId, name, value)


    fun composeSystemMessages(
        systemPrompt: String,
        approvedMemory: String,
        localKnowledgeContext: String,
        toolGuide: String?
    ): List<ChatMessage> = BrainContextComposer.compose(
        systemPrompt, approvedMemory, localKnowledgeContext, toolGuide
    )

    suspend fun retrieveKnowledge(query: String, currentMessages: List<ChatMessage>): String {
        val ownedHistory = chatStore.load().filter {
            it.ownerUserId == scope.userId && it.ownerFeatureId == scope.featureId
        }
        return knowledgeStore.retrieve(query, ownedHistory, currentMessages)
    }
}
