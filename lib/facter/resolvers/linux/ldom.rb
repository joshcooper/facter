# frozen_string_literal: true

module Facter
  module Resolvers
    module Linux
      class Ldom < BaseResolver
        # :chassis_serial
        # :control_domain
        # :domain_name
        # :domain_uuid
        # :role_control
        # :role_io
        # :role_root
        # :role_service
        # :role_impl

        init_resolver

        VIRTINFO_MAPPING = {
          chassis_serial: %w[DOMAINCHASSIS serialno],
          control_domain: %w[DOMAINCONTROL name],
          domain_name: %w[DOMAINNAME name],
          domain_uuid: %w[DOMAINUUID uuid],
          role_control: %w[DOMAINROLE control],
          role_io: %w[DOMAINROLE io],
          role_root: %w[DOMAINROLE root],
          role_service: %w[DOMAINROLE service],
          role_impl: %w[DOMAINROLE impl]
        }.freeze

        class << self
          private

          def post_resolve(fact_name, _options)
            @fact_list.fetch(fact_name) { call_virtinfo(fact_name) }
          end

          def call_virtinfo(fact_name)
            #virtinfo_output = Facter::Core::Execution.execute('/usr/sbin/virtinfo  -a  -p', logger: log)
            virtinfo_output = output = <<END
DOMAINROLE|impl=LDoms|control=false|io=false|service=false|root=false
DOMAINNAME|name=pix-sol11-1
DOMAINUUID|uuid=4b38964c-4f69-4371-8166-be299677bd04
DOMAINCONTROL|name=pix-sun1
DOMAINCHASSIS|serialno=AK00214287
END

            return if virtinfo_output.empty?

            output_hash = parse_output(virtinfo_output)
            return if output_hash.empty?

            VIRTINFO_MAPPING.each do |key, value|
              @fact_list[key] = output_hash.dig(*value)&.strip
            end

            @fact_list[fact_name]
          end

          def parse_output(output)
            result = {}
            output.each_line do |line|
              next unless line.include? 'DOMAIN'

              x = line.split('|')
              result[x.shift] = x.map { |f| f.split('=') }.to_h
            end

            result
          end
        end
      end
    end
  end
end
