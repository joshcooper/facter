# frozen_string_literal: true

module Facter
  module Utils
    # Sort nested hash.
    def self.sort_hash_by_key(hash, recursive: true, &block)
      hash.keys.sort(&block).each_with_object(hash.class.new) do |key, seed|
        seed[key] = hash[key]
        seed[key] = sort_hash_by_key(seed[key], recursive: true, &block) if recursive && seed[key].is_a?(Hash)

        seed
      end
    end

    # REMIND: rename this, it's used for both fact names and user queries.
    def self.join_user_query(segments)
      # REMIND: what if there are single or double quotes
      segments.map do |segment|
        if segment.include?('.')
          "\"#{segment}\""
        else
          segment
        end
      end.join('.')
    end

    # This is basically copied from puppet's sublookup
    SPECIAL = /['"\.]/

    # REMIND: rename this, it's used for both fact names and user queries.
    def self.split_user_query(key)
      if key.match(SPECIAL).nil?
        puts "split #{key} to [#{key}]"
        return [key]
      end
      segments = key.split(/(\s*"[^"]+"\s*|\s*'[^']+'\s*|[^'".]+)/)
      if segments.empty?
        # Only happens if the original key was an empty string
        raise ArgumentError, 'Syntax error'
      elsif segments.shift == ''
        count = segments.size
        raise ArgumentError, 'Syntax error' unless count > 0

        segments.keep_if { |seg| seg != '.' }
        raise ArgumentError, 'Syntax error' unless segments.size * 2 == count + 1
        segments.map! do |segment|
          segment.strip!
          if segment.start_with?('"', "'")
            segment[1..-2]
          elsif segment =~ /^(:?[+-]?[0-9]+)$/
            segment.to_i
          else
            segment
          end
        end
      else
        raise ArgumentError, 'Syntax error'
      end
      puts "split #{key} to [#{segments.join(', ')}]"
      segments
    end

    def self.deep_stringify_keys(object)
      case object
      when Hash
        object.each_with_object({}) do |(key, value), result|
          result[key.to_s] = deep_stringify_keys(value)
        end
      when Array
        object.map { |e| deep_stringify_keys(e) }
      else
        object
      end
    end

    def self.try_to_bool(value)
      case value.to_s
      when 'true'
        true
      when 'false'
        false
      else
        value
      end
    end

    def self.try_to_int(value)
      Integer(value)
    rescue ArgumentError, TypeError
      value
    end
  end
end
