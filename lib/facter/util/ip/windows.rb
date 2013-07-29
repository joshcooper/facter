# encoding: UTF-8

require 'facter/util/ip/base'
require 'facter/util/wmi'

class Facter::Util::IP::Windows < Facter::Util::IP::Base
  # A regex to match an IPv4 address from `ifconfig` output.
  #
  # @return [Regexp]
  #
  # @api private
  IPADDRESS_REGEX = /\s+IP\sAddress:\s+([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)/

  # A regex to match an IPv6 address from `ifconfig` output.
  #
  # @return [Regexp]
  #
  # @api private
  IPADDRESS6_REGEX = /Address\s((?![fe80|::1])(?>[0-9,a-f,A-F]*\:{1,2})+[0-9,a-f,A-F]{0,4})/

  # A regex to match the netmask from `ifconfig` output.
  #
  # @return [Regexp]
  #
  # @api private
  NETMASK_REGEX = /\s+Subnet\sPrefix:\s+\S+\s+\(mask\s([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)\)/

  # The path to netsh.exe.
  #
  # @return [String]
  #
  # @api private
  NETSH = "#{ENV['SYSTEMROOT']}/system32/netsh.exe"

  # The WMI query used to return ip information
  #
  # @return [String]
  #
  # @api private
  WMI_IP_INFO_QUERY = 'SELECT Description, ServiceName, IPAddress, IPConnectionMetric, InterfaceIndex, Index, IPSubnet, MACAddress, MTU, SettingID FROM Win32_NetworkAdapterConfiguration WHERE IPConnectionMetric IS NOT NULL AND IPEnabled = TRUE'

  WINDOWS_LABEL_WMI_MAP = {
      :ipaddress => 'IPAddress',
      :ipaddress6 => 'IPAddress',
      :macaddress => 'MACAddress',
      :netmask => 'IPSubnet'
  }

  def self.to_s
    'windows'
  end

  # Windows doesn't display netmask in hex.
  #
  # @return [Boolean] false by default
  #
  # @api private
  def self.convert_netmask_from_hex?
    false
  end

  # Retrieves a list of unique interfaces names.
  #
  # @return [Array]
  #
  # @api private
  def self.interfaces
    network_interfaces = []
    self.exec_wmi_ip_query do |nic_config|
      Facter::Util::WMI.execquery("SELECT * FROM Win32_NetworkAdapter WHERE Index = #{nic_config.Index}").each do |nic|
        network_interfaces << nic.NetConnectionId
      end
    end

    network_interfaces.uniq
  end

  # Get the value of an interface and label. For example, you may want to find
  # the MTU for eth0.
  #
  # @param interface [String] label [String]
  #
  # @return [String] or [NilClass]
  #
  # @api private
  def self.value_for_interface_and_label(interface, label)
    wmi_value = WINDOWS_LABEL_WMI_MAP[label.downcase.to_sym]
    label_value = nil
    Facter::Util::WMI.execquery("SELECT Index FROM Win32_NetworkAdapter WHERE NetConnectionID = '#{interface}'").each do |nic|
      Facter::Util::WMI.execquery("SELECT #{wmi_value} FROM Win32_NetworkAdapterConfiguration WHERE Index = #{nic.Index}").each do |nic_config|
        case label.downcase.to_sym
        when :ipaddress
          nic_config.IPAddress.any? do |addr|
            label_value = addr if valid_ipv4_address?(addr)
            label_value
          end
        when :ipaddress6
          nic_config.IPAddress.any? do |addr|
            label_value = addr if Facter::Util::IP::Windows.valid_ipv6_address?(addr)
            label_value
          end
        when :netmask
          nic_config.IPSubnet.any? do |addr|
            label_value = addr if Facter::Util::IP::Windows.valid_ipv4_address?(addr)
            label_value
          end
        when :macaddress
          label_value = nic_config.MACAddress
        end
      end
    end

    label_value
  end

  # Executes a wmi query that returns ip information and returns for each item in the results
  #
  # @return [Win32OLE] objects
  #
  # @api private
  def self.exec_wmi_ip_query(&block)
    Facter::Util::WMI.execquery(WMI_IP_INFO_QUERY).each do |nic|
      yield nic
    end
  end

  # Gets a list of active adapters and sorts by the lowest connection metric (aka best weight) and MACAddress to ensure order
  #
  # @return [Win32OLE]
  #
  # @api private
  def self.get_preferred_network_adapters
    network_adapters = []

    self.exec_wmi_ip_query do |nic|
      network_adapters << nic
    end

    require 'facter/util/registry'
    bindings = {}

    Facter::Util::Registry.hklm_read('SYSTEM\CurrentControlSet\Services\Tcpip\Linkage','Bind').each_with_index do |entry, index|
      match_data = entry.match(/\\Device\\({.*})/)
      unless match_data.nil?
        bindings[match_data[1]] = index
      end
    end

    network_adapters.sort do |nic_left,nic_right|
      cmp = nic_left.IPConnectionMetric <=> nic_right.IPConnectionMetric
      if cmp == 0
        bindings[nic_left.SettingID] <=> bindings[nic_right.SettingID]
      else
        cmp
      end
    end
  end

  # Determines if the value passed in is a valid ipv4 address.
  #
  # @param ip_address [String]
  #
  # @return [String] or [NilClass]
  #
  # @api private
  def self.valid_ipv4_address?(ip_address)
    String(ip_address).scan(/(?:[0-9]{1,3}\.){3}[0-9]{1,3}/).each do |match|
      # excluding 169.254.x.x in Windows - this is the DHCP APIPA
      #  meaning that if the node cannot get an ip address from the dhcp server,
      #  it auto-assigns a private ip address
      unless match == "127.0.0.1" or match =~ /^169.254.*/
        return match
      end
    end

    nil
  end

  # Determines if the value passed in is a valid ipv6 address.
  #
  # @param ip_address [String]
  #
  # @return [String] or [NilClass]
  #
  # @api private
  def self.valid_ipv6_address?(ip_address)
    String(ip_address).scan(/(?>[0-9,a-f,A-F]*\:{1,2})+[0-9,a-f,A-F]{0,4}/).each do |match|
      unless match =~ /fe80.*/ or match == "::1"
        return match
      end
    end

    nil
  end

end
