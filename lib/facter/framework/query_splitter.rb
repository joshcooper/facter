# frozen_string_literal: true

module Facter
  module Framework
    module QuerySplitter
      class << self
        SPECIAL = /['"\.]/.freeze

        def split_key(key)
          return [key] unless key =~ SPECIAL

          segments = key.split(/(\s*"[^"]+"\s*|\s*'[^']+'\s*|[^'".]+)/)

          # Only happens if the original key was an empty string
          return [key] if segments.empty?

          if segments.shift == ''
            count = segments.size
            return [key] unless count.positive?

            segments.keep_if { |seg| seg != '.' }
            return [key] unless segments.size * 2 == count + 1

            generate_segments!(segments)
          else
            [key]
          end
        end

        def generate_segments!(arr)
          arr.map! do |segment|
            segment.strip!
            if segment.start_with?('"', "'")
              segment[1..-2]
            elsif segment =~ /^(:?[+-]?[0-9]+)$/
              segment.to_i
            else
              segment
            end
          end
        end
      end
    end
  end
end
