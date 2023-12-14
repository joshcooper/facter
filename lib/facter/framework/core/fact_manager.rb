# frozen_string_literal: true

module Facter
  class FactManager
    def initialize(fact_loader: FactLoader.new,
                   fact_filter: FactFilter.new,
                   internal_fact_manager: InternalFactManager.new,
                   external_fact_manager: ExternalFactManager.new,
                   cache_manager: CacheManager.new,
                   options: Options.get)
      @fact_loader = fact_loader
      @fact_filter = fact_filter
      @internal_fact_mgr = internal_fact_manager
      @external_fact_mgr = external_fact_manager
      @cache_manager = cache_manager
      # REMIND: this is actually an OptionsStore
      @options = options
      @log = Log.new(self)
    end

    def resolve_facts(query_parser)
      log_resolving_method
      empty_query_list = query_parser.query_list.empty?
      searched_facts = query_parser.parse(@fact_loader.load(empty_query_list, @options))

      searched_facts, cached_facts = @cache_manager.resolve_facts(searched_facts)
      internal_facts = @internal_fact_mgr.resolve_facts(searched_facts)
      external_facts = @external_fact_mgr.resolve_facts(searched_facts)

      resolved_facts = override_core_facts(internal_facts, external_facts)

      resolved_facts.concat(cached_facts)
      @cache_manager.cache_facts(resolved_facts)

      @fact_filter.filter_facts!(resolved_facts, empty_query_list)

      log_resolved_facts(resolved_facts)
      resolved_facts
    end

    # resolve a fact by name, in a similar way that facter 3 does.
    # search is done in multiple steps, and the next step is executed
    # only if the previous one was not able to resolve the fact
    # - load the `fact_name.rb` from the configured custom directories
    # - load all the core facts, external facts and env facts
    # - load all custom facts
    def resolve_fact(query_parser, fact_name)
      log_resolving_method
      @log.debug("resolving fact with user_query: #{fact_name}")

      custom_facts = custom_fact_by_filename(query_parser, fact_name) || []
      core_and_external_facts = core_or_external_fact(query_parser, fact_name) || []
      resolved_facts = core_and_external_facts + custom_facts

      if resolved_facts.empty? || resolved_facts.none? { |rf| rf.resolves?(fact_name) }
        resolved_facts.concat(all_custom_facts(query_parser, fact_name))
      end

      @cache_manager.cache_facts(resolved_facts)

      log_resolved_facts(resolved_facts)
      resolved_facts
    end

    def resolve_core(query_parser, fact_name, options = {})
      log_resolving_method
      core_fact(query_parser, fact_name, options)
    end

    private

    def log_resolving_method
      if @options[:sequential]
        @log.debugonce('Resolving facts sequentially')
      else
        @log.debugonce('Resolving fact in parallel')
      end
    end

    def core_fact(query_parser, fact_name, options)
      empty_user_query = query_parser.query_list.empty?
      loaded_facts = @fact_loader.load_internal_facts(empty_user_query, options)

      searched_facts = query_parser.parse(loaded_facts)
      searched_facts, cached_facts = @cache_manager.resolve_facts(searched_facts)

      resolved_facts = @internal_fact_mgr.resolve_facts(searched_facts)
      resolved_facts.concat(cached_facts)

      @fact_filter.filter_facts!(resolved_facts, empty_user_query)

      resolved_facts
    end

    def custom_fact_by_filename(query_parser, fact_name)
      @log.debug("Searching fact: #{fact_name} in file: #{fact_name}.rb")

      custom_fact = @fact_loader.load_custom_fact(@options, fact_name)
      return unless custom_fact.any?

      searched_facts = query_parser.parse(custom_fact)
      searched_facts, cached_facts = @cache_manager.resolve_facts(searched_facts)

      resolved_facts = @external_fact_mgr.resolve_facts(searched_facts)
      resolved_facts.concat(cached_facts)
      resolved_facts if resolved_facts.any?
    end

    def core_or_external_fact(query_parser, fact_name)
      @log.debug("Searching fact: #{fact_name} in core facts and external facts")

      core_facts = core_fact(query_parser, fact_name, @options)
      external_facts = @fact_loader.load_external_facts(@options)
      searched_facts = query_parser.parse(external_facts)
      searched_facts, cached_facts = @cache_manager.resolve_facts(searched_facts)

      resolved_facts = @external_fact_mgr.resolve_facts(searched_facts)
      resolved_facts = override_core_facts(core_facts, resolved_facts)
      resolved_facts.concat(cached_facts)

      resolved_facts unless resolved_facts.map(&:value).compact.empty?
    end

    def all_custom_facts(query_parser, fact_name)
      @log.debug("Searching fact: #{fact_name} in all custom facts")

      custom_facts = @fact_loader.load_custom_facts(@options)
      searched_facts = query_parser.parse(custom_facts)
      searched_facts, cached_facts = @cache_manager.resolve_facts(searched_facts)

      resolved_facts = @external_fact_mgr.resolve_facts(searched_facts)
      resolved_facts.concat(cached_facts)
    end

    def override_core_facts(core_facts, custom_facts)
      return core_facts unless custom_facts

      custom_facts.each do |custom_fact|
        core_facts.delete_if { |core_fact| root_fact_name(core_fact) == custom_fact.name }
      end

      core_facts + custom_facts
    end

    def root_fact_name(fact)
      fact.name.split('.').first
    end

    def log_resolved_facts(resolved_facts)
      resolved_facts.each do |fact|
        @log.debug("fact \"#{fact.name}\" has resolved to: #{fact.value}") unless fact.value.nil?
      end
    end
  end
end
